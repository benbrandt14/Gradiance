# CLI and MCP server over the operation registry
Phase U · `docs/unified-surface-plan.md` § 3.4 (CLI row), § 8 (T3),
§ 10 row 8, § 13.3 (one decoder) · status: in review

Written by the orchestrator, 2026-11-18 (owner ruling "scoped work": a
session may write the brief when the work is in the plan). The
orchestrator also works it (one brief, one PR).

## Goal

Two new doorways, both thin clients of the remote surface (`--remote`,
registry-only since U-3b):

- `gradiance ops [PREFIX]` lists the operations; `gradiance op <id|name>
  [--field value]… [ARG]…` invokes one and prints its value as JSON.
- `gradiance mcp` serves the Model Context Protocol on stdio, so an MCP
  client (an agent session, the owner's desktop client) drives a running
  app through the same registry.

Neither doorway holds a list of operations. Both read the catalog from
the running app, so a new operation reaches them with no edit here.

## Facts (orchestrator, 2026-11-18, on `979f188`)

- `gradiance.ops` rows carry `id`, `fields` (declared order), `name`,
  `signature`, `doc`, `category`, `args`. No field types, no example.
  VM built-in rows have `id: null` (not callable remotely).
- `OpMeta::example` holds sample params for every operation (77 `META`
  declarations); `tests/it/op_conformance.rs` decodes them.
- No operation params struct uses `#[serde(default)]`: a field is
  required unless its type is `Option<_>`.
- Every operation id has a dot; no Lisp name has one.
- `bevy_remote`'s HTTP transport answers a JSON-RPC POST on `/`. The
  workspace has no HTTP client crate.

## Decisions (orchestrator, best estimate, recorded per the owner's
2026-11-17 ruling)

1. **Client, not host.** Both doorways talk to a running app over the
   existing HTTP transport. They do not start a headless Bevy app: the
   app owns the window, the screenshot path and the undo stack the owner
   sees. `gradiance mcp --launch` starts `gradiance --remote <port>` when
   nothing answers, and stops it when stdin closes.
2. **Home: `gradiance-script::client`**, beside `remote` (it shares the
   method names). No new crate, no new dependency edge, no new crate in
   the lockfile: the HTTP/1.1 client is a small `std::net` POST to
   127.0.0.1 (`Connection: close`, `Content-Length` or chunked body).
3. **One decoder holds.** The CLI turns flags into a named JSON object
   and calls the `id` form; positional args map by declared field order.
   Values are coerced by the field's schema type (a `string` field keeps
   the raw text; anything else is parsed as JSON, and `1,2` becomes
   `[1, 2]`). The server decodes with the registry's serde decoder, as
   for every doorway.
4. **Schema from reflection** (§ 3.4): `OpEntry::schema()` builds a JSON
   Schema object from the params' `TypeInfo`: numbers, integers,
   booleans, strings, `Option` (not required), lists and arrays; any
   other type is described by its short type path and documented by the
   operation's `example`. `gradiance.ops` rows gain `schema` and
   `example` (`category` already is the seam).
5. **MCP tools: `ops`, `query`, `execute`, `batch`**, not one tool per
   operation. § 6 adopts Craft's MCP names (`execute`, `batch`); one tool
   per operation would put ~80 schemas in every agent's context and fix
   the tool list at connect time. `ops` returns the schemas on demand.
   `query` maps to `gradiance.query` and is marked read-only, so a client
   can auto-approve it. `batch` runs calls in order and stops at the
   first error; it is **not** one undo step (each call is its own, as
   successive remote calls are).
6. **MCP protocol subset:** `initialize`, `notifications/*` (no reply),
   `ping`, `tools/list`, `tools/call`; newline-delimited JSON-RPC 2.0 on
   stdio; protocol version `2025-06-18`. A failed operation is a tool
   result with `isError: true` (MCP's rule), not a JSON-RPC error.

## Approach

1. `gradiance-op`: `OpEntry::schema()` from reflection; unit tests.
2. `remote.rs`: catalog rows gain `schema` and `example` (parsed).
3. `gradiance-script/src/client/`: `http` (POST, response parse),
   `Remote` trait (one method call) with the HTTP impl, `cli` (argument
   parse, flag coercion, output, exit codes), `mcp` (the stdio server,
   generic over `Remote`).
4. `src/main.rs`: `ops`, `op`, `mcp` as first argument dispatch to the
   client before any `App` is built.
5. Tests: unit tests for HTTP parse, flag coercion and the MCP handler
   against a fake `Remote`; `tests/it/remote.rs` gains the CLI doorway
   equivalence test (every operation's example, rendered as flags,
   parses back to the example against the live catalog), a schema test
   (schema properties equal the declared fields; each example value has
   its field's schema type), and an MCP round trip against the
   in-process method table.
6. Docs: `docs/scripting.md` § Remote control gains the CLI and MCP
   forms; `CLAUDE.md` build section names the commands.

## Out of scope

- One MCP tool per operation, MCP resources and prompts, image content
  for `ui.screenshot` (the tool returns the path).
- Authentication, non-localhost binding, a headless host mode.
- New operations, a batch-as-one-undo-step command (`command.batch`).
- The follow-ups the coordinator named (colour chip on a mixed group,
  `bodies-where` label filter, fast-drag offset): retained in
  `docs/state.md`.

## Done when

- `gradiance ops`, `gradiance op`, `gradiance mcp` work against
  `gradiance --remote`; `--port` and `GRADIANCE_PORT` select the port.
- The tests above pass; fmt, clippy, doc, `tests/boundaries.rs` green
  locally; CI green.
- Outcome filled; `docs/state.md` updated.

## Outcome

Orchestrator as worker, 2026-11-18, on `979f188`.

**Landed.**
- `gradiance-op`: `schema.rs` builds a params JSON Schema from reflection
  (`OpEntry::schema()`); `OpEntry::example()` parses `OpMeta::example`.
  `gradiance.ops` rows carry both.
- `gradiance-script/src/client/` (new): `http` (std-only POST + response
  parse, chunked included), `Remote` trait and `HttpRemote`, `cli`
  (`ops`, `op`; exit codes 0/1/2/3), `mcp` (stdio server, four tools).
  `src/main.rs` dispatches `ops`/`op`/`mcp` before any `App` is built.
- Tests: 22 unit tests in `gradiance-script` (`client::`), 3 in
  `gradiance-op` (`schema::`), 5 in `tests/it/client.rs`: schema round
  trip and CLI-flag doorway equivalence over **every** catalog operation,
  MCP execute/query/ops against the in-process method table, and one
  real HTTP round trip against Bevy's transport.
- Docs: `docs/scripting.md` § Command line and MCP; `CLAUDE.md` build
  block; two `decision: true` entries in `docs/review/archive.ron`
  (`mcp-tool-shape`, `cli-client-or-host`).

**Measured.** All example params of all registry operations round-trip
through CLI flags; every example value has its field's schema type. The
binary prints usage and exits 3 with a "start one with `gradiance
--remote`" message when no app answers.

**Not checked.** `gradiance mcp --launch` against a real windowed app
(the container has no display in this run), and a real MCP client
(Claude Code, Claude Desktop) connecting. The protocol subset follows
the MCP 2025-06-18 stdio transport; a client that needs `resources/*` or
`prompts/*` gets "method not served".

**Checked locally** (rustc 1.100.0): `cargo fmt --all -- --check`;
`cargo clippy --workspace --all-targets -- -D warnings`;
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --lib`;
`tests/boundaries.rs`; `-p gradiance-op`, `-p gradiance-script client`,
`--test it client:: remote:: op_conformance::`. The workspace run is CI's.

**For later.** An MCP image result for `ui.screenshot`; a batch that is
one undo step (`command.batch`) if agents need atomic edits.
