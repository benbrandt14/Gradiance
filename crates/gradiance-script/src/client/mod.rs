//! The client doorways: the command line (`gradiance ops`, `gradiance op`)
//! and the MCP server (`gradiance mcp`), both thin clients of the remote
//! surface ([`crate::remote`]) of a running app (brief U-6).
//!
//! Neither holds a list of operations. Both read the catalog
//! (`gradiance.ops`: id, fields, params schema, example) from the app they
//! talk to and invoke by id, so the server's one serde decoder decides what
//! is accepted (`docs/unified-surface-plan.md` § 13.3) and a new operation
//! reaches both with no edit here.
//!
//! - [`http`] — the one HTTP exchange: a JSON POST to 127.0.0.1.
//! - [`cli`] — argument parsing, flag coercion by schema type, output.
//! - [`mcp`] — the Model Context Protocol on stdio, generic over [`Remote`]
//!   so tests drive it against the in-process method table.
//!
//! None of this builds a Bevy app: the binary dispatches to [`main`] before
//! it creates one.

pub mod cli;
pub mod http;
pub mod mcp;

use serde_json::{Value, json};
use std::io;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The environment variable that names the port when `--port` does not.
pub const PORT_ENV: &str = "GRADIANCE_PORT";

/// How long `gradiance mcp --launch` waits for a started app to answer.
pub const LAUNCH_TIMEOUT: Duration = Duration::from_secs(90);

/// Why one remote call gave no value.
#[derive(Debug, thiserror::Error)]
pub enum CallError {
    /// Nothing answers on the port.
    #[error(
        "no Gradiance app answers on 127.0.0.1:{port} ({why}); start one with `gradiance --remote {port}`"
    )]
    Unreachable {
        /// The port tried.
        port: u16,
        /// The socket error.
        why: String,
    },
    /// The reply is not a JSON-RPC reply.
    #[error("bad reply from the app: {0}")]
    Protocol(String),
    /// The app refused the call (unknown operation, params that do not
    /// decode, an operation that refused its params).
    #[error("{message}")]
    Rpc {
        /// The JSON-RPC error code.
        code: i64,
        /// The app's message.
        message: String,
    },
}

/// One JSON-RPC method call on the remote surface. The HTTP client
/// implements it; tests implement it over the in-process method table.
pub trait Remote {
    /// Calls `method` with `params`; the reply's `result`.
    fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError>;
}

/// The remote surface of an app on a local port, over HTTP.
#[derive(Debug)]
pub struct HttpRemote {
    /// The local port.
    pub port: u16,
    next_id: u64,
}

impl HttpRemote {
    /// A client for the app on `port`.
    pub fn new(port: u16) -> Self {
        Self { port, next_id: 1 }
    }
}

impl Remote for HttpRemote {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let (status, body) =
            http::post(self.port, request.to_string().as_bytes()).map_err(|err| {
                match err.kind() {
                    io::ErrorKind::InvalidData => CallError::Protocol(err.to_string()),
                    _ => CallError::Unreachable {
                        port: self.port,
                        why: err.to_string(),
                    },
                }
            })?;
        let reply: Value = serde_json::from_slice(&body).map_err(|err| {
            CallError::Protocol(format!(
                "HTTP {status}, body is not JSON ({err}): {:.120}",
                String::from_utf8_lossy(&body)
            ))
        })?;
        rpc_result(reply)
    }
}

/// The `result` of a JSON-RPC reply, or its `error` as a [`CallError`].
pub fn rpc_result(mut reply: Value) -> Result<Value, CallError> {
    if let Some(error) = reply.get("error").filter(|e| !e.is_null()) {
        return Err(CallError::Rpc {
            code: error["code"].as_i64().unwrap_or(0),
            message: error["message"]
                .as_str()
                .map_or_else(|| error.to_string(), str::to_owned),
        });
    }
    match reply.get_mut("result") {
        Some(result) => Ok(result.take()),
        None => Err(CallError::Protocol(format!(
            "no `result` or `error`: {:.120}",
            reply.to_string()
        ))),
    }
}

/// The client subcommands; any other first argument starts the app.
pub const SUBCOMMANDS: &[&str] = &["ops", "op", "mcp"];

/// Runs a client subcommand if `args` (without the program name) starts
/// with one, and returns the process exit code; `None` means "start the
/// app". Removes `--port N` / `--port=N` wherever it appears.
pub fn main(args: &[String]) -> Option<i32> {
    let command = args.first()?;
    if !SUBCOMMANDS.contains(&command.as_str()) {
        return None;
    }
    let (port, rest) = match take_port(&args[1..]) {
        Ok(found) => found,
        Err(why) => {
            eprintln!("gradiance {command}: {why}");
            return Some(cli::EXIT_USAGE);
        }
    };
    let mut remote = HttpRemote::new(port);
    if command == "mcp" {
        return Some(run_mcp(&mut remote, &rest));
    }
    let mut out = io::stdout().lock();
    let mut err = io::stderr().lock();
    Some(cli::run(command, &rest, &mut remote, &mut out, &mut err))
}

/// `gradiance mcp [--launch]`: serves MCP on stdio until stdin closes.
fn run_mcp(remote: &mut HttpRemote, args: &[String]) -> i32 {
    let mut launch = false;
    for arg in args {
        match arg.as_str() {
            "--launch" => launch = true,
            "-h" | "--help" => {
                eprintln!("{}", mcp::USAGE);
                return 0;
            }
            other => {
                eprintln!("gradiance mcp: unknown argument `{other}`\n{}", mcp::USAGE);
                return cli::EXIT_USAGE;
            }
        }
    }
    let child = if launch {
        match ensure_app(remote.port) {
            Ok(child) => child,
            Err(why) => {
                eprintln!("gradiance mcp --launch: {why}");
                return cli::EXIT_REMOTE;
            }
        }
    } else {
        None
    };
    let served = mcp::serve(remote, io::stdin().lock(), &mut io::stdout().lock());
    if let Some(mut child) = child {
        // Started here, so stopped here: the client closed the session.
        let _ = child.kill();
        let _ = child.wait();
    }
    match served {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("gradiance mcp: {err}");
            cli::EXIT_REMOTE
        }
    }
}

/// Starts `gradiance --remote <port>` unless an app already answers there,
/// and waits until it does. Its stdout is discarded: in MCP mode stdout
/// carries the protocol.
fn ensure_app(port: u16) -> Result<Option<Child>, String> {
    if answers(port) {
        return Ok(None);
    }
    let exe = std::env::current_exe().map_err(|err| format!("no executable path: {err}"))?;
    let mut child = Command::new(exe)
        .args(["--remote", &port.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|err| format!("could not start the app: {err}"))?;
    let start = Instant::now();
    while start.elapsed() < LAUNCH_TIMEOUT {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("the app exited at start-up ({status})"));
        }
        if answers(port) {
            return Ok(Some(child));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let _ = child.kill();
    Err(format!(
        "the app did not answer on port {port} within {} s",
        LAUNCH_TIMEOUT.as_secs()
    ))
}

/// Whether something accepts connections on the local `port`.
fn answers(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(&http::local(port), http::CONNECT_TIMEOUT).is_ok()
}

/// Splits `--port N` / `--port=N` out of `args`; the port defaults to
/// [`PORT_ENV`], then to [`crate::remote::DEFAULT_PORT`].
pub fn take_port(args: &[String]) -> Result<(u16, Vec<String>), String> {
    let mut port = None;
    let mut rest = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let value = if arg == "--port" {
            Some(iter.next().ok_or("`--port` needs a number")?.clone())
        } else {
            arg.strip_prefix("--port=").map(str::to_owned)
        };
        match value {
            Some(value) => {
                port = Some(
                    value
                        .parse::<u16>()
                        .map_err(|_| format!("`{value}` is not a port"))?,
                );
            }
            None => rest.push(arg.clone()),
        }
    }
    let port = match port {
        Some(port) => port,
        None => match std::env::var(PORT_ENV) {
            Ok(value) => value
                .parse::<u16>()
                .map_err(|_| format!("{PORT_ENV}=`{value}` is not a port"))?,
            Err(_) => crate::remote::DEFAULT_PORT,
        },
    };
    Ok((port, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn port_flag_is_taken_from_anywhere() {
        let (port, rest) =
            take_port(&strings(&["a", "--port", "15800", "b"])).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((port, rest), (15800, strings(&["a", "b"])));
        let (port, _) = take_port(&strings(&["--port=9"])).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(port, 9);
        assert!(take_port(&strings(&["--port"])).is_err());
        assert!(take_port(&strings(&["--port", "x"])).is_err());
    }

    #[test]
    fn a_reply_is_its_result_or_its_error() {
        let ok = rpc_result(json!({"jsonrpc": "2.0", "id": 1, "result": [1]}));
        assert_eq!(ok.ok(), Some(json!([1])));
        let err = rpc_result(json!({"id": 1, "error": {"code": -32602, "message": "no"}}));
        assert!(
            matches!(err, Err(CallError::Rpc { code: -32602, ref message }) if message == "no")
        );
        assert!(matches!(
            rpc_result(json!({"id": 1})),
            Err(CallError::Protocol(_))
        ));
    }

    #[test]
    fn other_first_arguments_start_the_app() {
        assert_eq!(main(&strings(&["scene.ron"])), None);
        assert_eq!(main(&strings(&["--remote"])), None);
        assert_eq!(main(&[]), None);
    }
}
