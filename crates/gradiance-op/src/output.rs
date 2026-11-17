//! [`OpOutput`]: what an operation lowers to, and [`apply`], the one place
//! that writes it.

use crate::reflect_path::{PathError, write_path};
use bevy::ecs::component::Mutable;
use bevy::prelude::*;
use serde_json::Value;
use std::fmt::Debug;
use std::marker::PhantomData;

/// One write an operation's seam takes. Private: an [`OpOutput`] can only hold
/// the kinds its constructors build, so an operation cannot smuggle in an
/// arbitrary world mutation.
trait Write: Send + Sync {
    /// A stable text form, for the conformance tests and the trace.
    fn describe(&self) -> String;
    fn apply(self: Box<Self>, world: &mut World);
}

/// An intent (or other message) written to its existing queue.
struct MessageWrite<T>(T);

impl<T: Message + Debug> Write for MessageWrite<T> {
    fn describe(&self) -> String {
        format!("{:?}", self.0)
    }

    fn apply(self: Box<Self>, world: &mut World) {
        world.write_message(self.0);
    }
}

/// One scalar written by reflect-path onto a settings resource (the
/// invariant-4 config seam).
struct SettingWrite<R> {
    path: String,
    value: f64,
    resource: PhantomData<fn() -> R>,
}

impl<R: Resource<Mutability = Mutable> + Reflect> Write for SettingWrite<R> {
    fn describe(&self) -> String {
        format!(
            "{}.{} = {}",
            std::any::type_name::<R>(),
            self.path,
            self.value
        )
    }

    fn apply(self: Box<Self>, world: &mut World) {
        // Already trialled on the mirror in `OpOutput::setting`; a failure
        // here means the resource and its mirror disagree on shape.
        if let Some(mut resource) = world.get_resource_mut::<R>()
            && let Err(err) = write_path(&mut *resource, &self.path, self.value)
        {
            warn!("setting write: {err}");
        }
    }
}

/// The result of [`Operation::lower`](crate::Operation::lower): zero or more
/// writes for [`apply`] to make, plus an optional value to return to the
/// caller. One shape covers all four seams: an edit holds one intent, a query
/// holds only a value, `spawn-box` holds an intent **and** the new handle, a
/// config operation holds a setting write.
#[derive(Default)]
pub struct OpOutput {
    writes: Vec<Box<dyn Write>>,
    value: Option<Value>,
}

impl OpOutput {
    /// No write and no value: the operation had nothing to do.
    pub fn none() -> Self {
        Self::default()
    }

    /// Adds a message for its existing queue (an intent for the command
    /// dispatcher, a request for the layer that drains it).
    #[must_use]
    pub fn message<T: Message + Debug>(mut self, message: T) -> Self {
        self.writes.push(Box::new(MessageWrite(message)));
        self
    }

    /// Adds a write of `value` at reflect-path `path` on the settings resource
    /// `R`. The write is trialled on `mirror` first, so a bad path is an error
    /// here and never a write that silently does not land.
    pub fn setting<R: Resource<Mutability = Mutable> + Reflect + Clone>(
        mut self,
        mirror: &R,
        path: &str,
        value: f64,
    ) -> Result<Self, PathError> {
        let mut trial = mirror.clone();
        write_path(&mut trial, path, value)?;
        self.writes.push(Box::new(SettingWrite::<R> {
            path: path.to_owned(),
            value,
            resource: PhantomData,
        }));
        Ok(self)
    }

    /// Sets the value the caller gets back.
    #[must_use]
    pub fn returning(mut self, value: impl Into<Value>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// The value the caller gets back, if any.
    pub fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }

    /// Whether the output holds no write.
    pub fn writes_nothing(&self) -> bool {
        self.writes.is_empty()
    }

    /// A stable text form of every write, in order. Two doorways lowered one
    /// call to the same output exactly when these (and the values) are equal.
    pub fn describe(&self) -> Vec<String> {
        self.writes.iter().map(|w| w.describe()).collect()
    }
}

/// One applied (or, in a dry run, withheld) output.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceEntry {
    /// The operation's id.
    pub id: &'static str,
    /// [`OpOutput::describe`] of the output.
    pub writes: Vec<String>,
    /// The output's return value.
    pub value: Option<Value>,
}

/// A record of every output that reached [`apply`], kept while this resource
/// exists. The conformance tests insert it to compare doorways.
#[derive(Resource, Debug, Default)]
pub struct OpTrace {
    /// While `true`, [`apply`] records an output and does **not** write it, so
    /// several doorways can lower against one unchanged scene.
    pub dry_run: bool,
    /// The outputs seen, oldest first.
    pub entries: Vec<TraceEntry>,
}

/// Writes one output through the seams it names and returns its value. Every
/// doorway ends here; it is the registry's only world mutation.
pub fn apply(world: &mut World, id: &'static str, output: OpOutput) -> Option<Value> {
    if let Some(mut trace) = world.get_resource_mut::<OpTrace>() {
        trace.entries.push(TraceEntry {
            id,
            writes: output.describe(),
            value: output.value.clone(),
        });
        if trace.dry_run {
            return output.value;
        }
    }
    for write in output.writes {
        write.apply(world);
    }
    output.value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Message, Debug, Clone, PartialEq)]
    struct Ping(u32);

    #[derive(Resource, Reflect, Clone, Default)]
    struct Knobs {
        speed: f32,
    }

    #[test]
    fn apply_writes_messages_and_settings() {
        let mut app = App::new();
        app.add_message::<Ping>();
        app.init_resource::<Knobs>();
        let output = OpOutput::none()
            .message(Ping(3))
            .setting(&Knobs::default(), "speed", 2.0)
            .unwrap_or_default()
            .returning(5);
        assert_eq!(output.describe().len(), 2);
        let value = apply(app.world_mut(), "test.op", output);
        assert_eq!(value, Some(Value::from(5)));
        assert_eq!(app.world().resource::<Messages<Ping>>().len(), 1);
        assert!((app.world().resource::<Knobs>().speed - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_bad_setting_path_fails_before_any_write() {
        let result = OpOutput::none().setting(&Knobs::default(), "sped", 2.0);
        assert!(result.is_err());
    }

    #[test]
    fn a_dry_run_records_and_writes_nothing() {
        let mut app = App::new();
        app.add_message::<Ping>();
        app.insert_resource(OpTrace {
            dry_run: true,
            entries: Vec::new(),
        });
        apply(
            app.world_mut(),
            "test.op",
            OpOutput::none().message(Ping(1)),
        );
        assert_eq!(app.world().resource::<Messages<Ping>>().len(), 0);
        let trace = app.world().resource::<OpTrace>();
        assert_eq!(trace.entries[0].writes, vec!["Ping(1)".to_owned()]);
    }
}
