//! The review-mode formats: open product **questions** the owner answers
//! inside the running app, and the **answers** file the app writes back
//! (brief 4b-0, `docs/orchestration.md` § Review mode).
//!
//! Three files, one schema family, all RON:
//!
//! - **questions** (`docs/review/questions.ron`) — [`QuestionSet`]: each
//!   [`Question`] carries 2–4 [`ReviewOption`]s, and each option is a live
//!   scene (a `.ron` scene and/or `.scm` scripts) plus optional
//!   [`SettingsOverride`]s. Written by orchestrators and workers.
//! - **answers** (`docs/review/answers.ron`) — [`AnswerSet`]: one [`Answer`]
//!   per question, written by the app, committed by the owner.
//! - **archive** (`docs/review/archive.ron`) — [`Archive`]: ingested
//!   questions with their answers, kept for provenance.
//!
//! This module is pure: parse, validate, encode, and merge. Disk IO (the
//! atomic answers write, the git revision, the clock) is
//! `gradiance-persist`'s, and loading an option into the world is the app
//! shell's (`gradiance::review`).

use crate::records::EnvironmentRecord;
use gradiance_domain::settings::{LightingSettings, RenderSettings, ScenerySettings, SimSettings};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Version of the questions / answers / archive schema this build reads.
pub const REVIEW_FORMAT_VERSION: u32 = 1;

/// Fewest options a question may offer.
pub const MIN_OPTIONS: usize = 2;
/// Most options a question may offer.
pub const MAX_OPTIONS: usize = 4;

/// What is wrong with a questions, answers or archive file.
#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    /// Filesystem failure (read, temp write, rename).
    #[error("{path}: {source}")]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The text is not valid RON for the schema (the message carries the
    /// line and column).
    #[error("{0}")]
    Parse(#[from] ron::de::SpannedError),
    /// Serialization failed.
    #[error("serialize: {0}")]
    Serialize(#[from] ron::Error),
    /// The file's schema version is not [`REVIEW_FORMAT_VERSION`].
    #[error("review file version {0} is not supported (this build reads {REVIEW_FORMAT_VERSION})")]
    Version(u32),
    /// A question breaks a structural rule.
    #[error("question `{id}`: {problem}")]
    Invalid {
        /// The offending question's id (or its index when the id is empty).
        id: String,
        /// What rule it breaks.
        problem: String,
    },
}

/// The questions file: every open question, in presentation order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionSet {
    /// Schema version ([`REVIEW_FORMAT_VERSION`]).
    pub version: u32,
    /// The open questions.
    pub questions: Vec<Question>,
}

/// One open product decision, presented as live scenes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Question {
    /// Stable identifier: lower-case ASCII letters, digits and `-`. The
    /// answers file keys on it, so it never changes once asked.
    pub id: String,
    /// One-line title, shown in the question list.
    pub title: String,
    /// Why it matters and what each choice implies.
    pub description: String,
    /// Where the question came from (a brief, a `state.md` item) — read by
    /// the orchestrator at ingestion.
    #[serde(default)]
    pub source: String,
    /// Whether the options may be shown **side by side** in one world.
    /// Only for options that differ in their bodies: settings are global
    /// resources, so a side-by-side question may not carry
    /// [`SettingsOverride`]s (validation rejects it).
    #[serde(default)]
    pub side_by_side: bool,
    /// The choices, 2–4.
    pub options: Vec<ReviewOption>,
}

/// One choice and the scene that demonstrates it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewOption {
    /// Short label ("Empty", "Ground + bodies"); unique within the question.
    /// The answer records it.
    pub label: String,
    /// One or two sentences: what this option is, what to try in it.
    pub caption: String,
    /// A scene file (`.ron`), relative to the questions file. Loaded first;
    /// without one the option starts from an empty scene with default
    /// scene settings.
    #[serde(default)]
    pub scene: Option<PathBuf>,
    /// Scripts (`.scm`), relative to the questions file, run in order —
    /// **one frame apart**, so a later script can index the bodies an
    /// earlier one spawned (one snapshot per run).
    #[serde(default)]
    pub scripts: Vec<PathBuf>,
    /// Scene-settings overrides applied after the scene and scripts.
    #[serde(default)]
    pub settings: SettingsOverride,
}

/// Replacement scene-content settings for one option.
///
/// Each present field **replaces** that resource; a field omitted inside
/// it takes its default (the settings structs are `#[serde(default)]`), so
/// `lighting: (ssao: true)` means "default lighting, SSAO on".
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsOverride {
    /// Simulation tuning.
    pub sim: Option<SimSettings>,
    /// Rendering style.
    pub render: Option<RenderSettings>,
    /// Lighting.
    pub lighting: Option<LightingSettings>,
    /// Back plane / ground scenery, perspective.
    pub scenery: Option<ScenerySettings>,
}

impl SettingsOverride {
    /// Whether this overrides nothing.
    pub fn is_empty(&self) -> bool {
        self.sim.is_none()
            && self.render.is_none()
            && self.lighting.is_none()
            && self.scenery.is_none()
    }

    /// Writes the overrides into an environment record (the shape a load
    /// applies), leaving every other field alone.
    pub fn apply_to(&self, environment: &mut EnvironmentRecord) {
        if let Some(sim) = &self.sim {
            environment.sim = sim.clone();
        }
        if let Some(render) = &self.render {
            environment.render = render.clone();
        }
        if let Some(lighting) = &self.lighting {
            environment.lighting = lighting.clone();
        }
        if let Some(scenery) = &self.scenery {
            environment.scenery = scenery.clone();
        }
    }
}

impl ReviewOption {
    /// The option's scene path resolved against `base` (the questions
    /// file's directory).
    pub fn scene_path(&self, base: &Path) -> Option<PathBuf> {
        self.scene.as_ref().map(|p| base.join(p))
    }

    /// The option's script paths resolved against `base`, in run order.
    pub fn script_paths(&self, base: &Path) -> Vec<PathBuf> {
        self.scripts.iter().map(|p| base.join(p)).collect()
    }
}

impl Question {
    /// The option labelled `label`.
    pub fn option(&self, label: &str) -> Option<&ReviewOption> {
        self.options.iter().find(|o| o.label == label)
    }

    /// Checks the structural rules; the first broken one is the error.
    pub fn validate(&self) -> Result<(), ReviewError> {
        let fail = |problem: String| ReviewError::Invalid {
            id: self.id.clone(),
            problem,
        };
        if !is_valid_id(&self.id) {
            return Err(fail(
                "the id must be non-empty lower-case ASCII letters, digits and `-`".into(),
            ));
        }
        if self.title.trim().is_empty() {
            return Err(fail("the title is empty".into()));
        }
        if !(MIN_OPTIONS..=MAX_OPTIONS).contains(&self.options.len()) {
            return Err(fail(format!(
                "has {} options; a question offers {MIN_OPTIONS} to {MAX_OPTIONS}",
                self.options.len()
            )));
        }
        for (i, option) in self.options.iter().enumerate() {
            if option.label.trim().is_empty() {
                return Err(fail(format!("option {i} has an empty label")));
            }
            if self.options[..i].iter().any(|o| o.label == option.label) {
                return Err(fail(format!("two options are labelled `{}`", option.label)));
            }
            if option.scene.is_none() && option.scripts.is_empty() {
                return Err(fail(format!(
                    "option `{}` has no scene: give it a `scene` (.ron) or `scripts` (.scm)",
                    option.label
                )));
            }
            if let Some(scene) = &option.scene
                && !has_extension(scene, "ron")
            {
                return Err(fail(format!(
                    "option `{}`: scene `{}` is not a .ron file",
                    option.label,
                    scene.display()
                )));
            }
            if let Some(script) = option.scripts.iter().find(|s| !has_extension(s, "scm")) {
                return Err(fail(format!(
                    "option `{}`: script `{}` is not a .scm file",
                    option.label,
                    script.display()
                )));
            }
            if self.side_by_side && !option.settings.is_empty() {
                return Err(fail(format!(
                    "option `{}` overrides settings, which are global — a question \
                     with settings overrides cannot be side_by_side",
                    option.label
                )));
            }
        }
        Ok(())
    }
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension().is_some_and(|e| e == ext)
}

/// Whether `id` is a valid question id.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl QuestionSet {
    /// Index of the question with `id`.
    pub fn position(&self, id: &str) -> Option<usize> {
        self.questions.iter().position(|q| q.id == id)
    }

    /// Checks every question and that ids are unique.
    pub fn validate(&self) -> Result<(), ReviewError> {
        if self.version != REVIEW_FORMAT_VERSION {
            return Err(ReviewError::Version(self.version));
        }
        for (i, question) in self.questions.iter().enumerate() {
            question.validate()?;
            if self.questions[..i].iter().any(|q| q.id == question.id) {
                return Err(ReviewError::Invalid {
                    id: question.id.clone(),
                    problem: "the id is used by an earlier question".into(),
                });
            }
        }
        Ok(())
    }
}

/// The owner's choice for one question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Choice {
    /// One of the offered options, by label.
    Picked(String),
    /// None of the offered options; the owner's own answer instead.
    NoneOfThese(String),
}

impl Choice {
    /// A one-line rendering for the UI.
    pub fn summary(&self) -> String {
        match self {
            Self::Picked(label) => label.clone(),
            Self::NoneOfThese(text) if text.trim().is_empty() => "none of these".into(),
            Self::NoneOfThese(text) => format!("none of these: {text}"),
        }
    }
}

/// One answered question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    /// The question's id.
    pub question: String,
    /// The question's title when it was answered (for a human reading the
    /// file; the id is the key).
    #[serde(default)]
    pub title: String,
    /// What the owner chose.
    pub choice: Choice,
    /// Free-text note (may be empty).
    #[serde(default)]
    pub note: String,
    /// When, as an RFC 3339 UTC timestamp (`2026-11-16T12:00:00Z`).
    pub answered_at: String,
    /// The checkout's git revision when answered, if git was available.
    #[serde(default)]
    pub revision: Option<String>,
}

/// The answers file: at most one entry per question, in first-answered
/// order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerSet {
    /// Schema version ([`REVIEW_FORMAT_VERSION`]).
    pub version: u32,
    /// The answers.
    pub answers: Vec<Answer>,
}

impl Default for AnswerSet {
    fn default() -> Self {
        Self {
            version: REVIEW_FORMAT_VERSION,
            answers: Vec::new(),
        }
    }
}

impl AnswerSet {
    /// The answer to question `id`, if any.
    pub fn get(&self, id: &str) -> Option<&Answer> {
        self.answers.iter().find(|a| a.question == id)
    }

    /// Records `answer`: replaces the earlier answer to the same question in
    /// place (one entry per question — the file's git history keeps the old
    /// one), or appends. Every other entry is untouched.
    pub fn upsert(&mut self, answer: Answer) {
        match self
            .answers
            .iter_mut()
            .find(|a| a.question == answer.question)
        {
            Some(slot) => *slot = answer,
            None => self.answers.push(answer),
        }
    }
}

/// One ingested question, kept for provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchiveEntry {
    /// The question as it was asked.
    pub question: Question,
    /// The owner's answer.
    pub answer: Answer,
    /// What the answer became: the brief, roadmap or `state.md` change.
    #[serde(default)]
    pub ingested_as: String,
}

/// The archive file: answered questions the orchestrator has ingested.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Archive {
    /// Schema version ([`REVIEW_FORMAT_VERSION`]).
    pub version: u32,
    /// Ingested questions, oldest first.
    pub entries: Vec<ArchiveEntry>,
}

/// RON options for reading review files: `implicit_some` lets authors write
/// `lighting: (ssao: true)` rather than `lighting: Some((ssao: true))`.
fn options() -> ron::Options {
    ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
}

fn pretty() -> ron::ser::PrettyConfig {
    ron::ser::PrettyConfig::new().struct_names(false)
}

/// Parses and validates a questions file.
pub fn questions_from_ron(text: &str) -> Result<QuestionSet, ReviewError> {
    let set: QuestionSet = options().from_str(text)?;
    set.validate()?;
    Ok(set)
}

/// Serializes a questions file.
pub fn questions_to_ron(set: &QuestionSet) -> Result<String, ReviewError> {
    Ok(options().to_string_pretty(set, pretty())?)
}

/// Parses an answers file. An empty (or whitespace-only) file is an empty
/// set, so a freshly created file needs no boilerplate.
pub fn answers_from_ron(text: &str) -> Result<AnswerSet, ReviewError> {
    if text.trim().is_empty() {
        return Ok(AnswerSet::default());
    }
    let set: AnswerSet = options().from_str(text)?;
    if set.version != REVIEW_FORMAT_VERSION {
        return Err(ReviewError::Version(set.version));
    }
    Ok(set)
}

/// Serializes an answers file (deterministic: same set, same bytes; ends
/// in a newline, so the committed file diffs cleanly).
pub fn answers_to_ron(set: &AnswerSet) -> Result<String, ReviewError> {
    let mut text = options().to_string_pretty(set, pretty())?;
    text.push('\n');
    Ok(text)
}

/// Parses an archive file.
pub fn archive_from_ron(text: &str) -> Result<Archive, ReviewError> {
    let archive: Archive = options().from_str(text)?;
    if archive.version != REVIEW_FORMAT_VERSION {
        return Err(ReviewError::Version(archive.version));
    }
    Ok(archive)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(label: &str) -> ReviewOption {
        ReviewOption {
            label: label.into(),
            caption: format!("{label} caption"),
            scene: None,
            scripts: vec![PathBuf::from(format!("{label}.scm"))],
            settings: SettingsOverride::default(),
        }
    }

    fn question(id: &str) -> Question {
        Question {
            id: id.into(),
            title: "A title".into(),
            description: "Why".into(),
            source: String::new(),
            side_by_side: false,
            options: vec![option("a"), option("b")],
        }
    }

    fn set(questions: Vec<Question>) -> QuestionSet {
        QuestionSet {
            version: REVIEW_FORMAT_VERSION,
            questions,
        }
    }

    fn answer(id: &str, label: &str) -> Answer {
        Answer {
            question: id.into(),
            title: "A title".into(),
            choice: Choice::Picked(label.into()),
            note: String::new(),
            answered_at: "2026-11-16T12:00:00Z".into(),
            revision: Some("04067ab".into()),
        }
    }

    #[test]
    fn questions_round_trip() {
        let mut q = question("startup-scene");
        q.options[1].settings.lighting = Some(LightingSettings {
            ssao: true,
            ..LightingSettings::default()
        });
        q.options[0].scene = Some("scenes/a.ron".into());
        let original = set(vec![q, question("other")]);
        let text = questions_to_ron(&original).unwrap();
        assert_eq!(questions_from_ron(&text).unwrap(), original);
    }

    #[test]
    fn a_partial_settings_override_defaults_the_rest() {
        let text = r#"(
            version: 1,
            questions: [(
                id: "look",
                title: "Look",
                description: "d",
                options: [
                    (label: "a", caption: "c", scripts: ["a.scm"]),
                    (label: "b", caption: "c", scripts: ["a.scm"],
                     settings: (lighting: (ssao: true), render: (toon_enabled: false))),
                ],
            )],
        )"#;
        let parsed = questions_from_ron(text).unwrap();
        let settings = &parsed.questions[0].options[1].settings;
        let lighting = settings.lighting.as_ref().unwrap();
        assert!(lighting.ssao);
        assert_eq!(lighting.lights, LightingSettings::default().lights);
        let render = settings.render.as_ref().unwrap();
        assert!(!render.toon_enabled);
        assert_eq!(render.bands, RenderSettings::default().bands);
        assert!(settings.sim.is_none());
    }

    #[test]
    fn a_syntax_error_names_the_line() {
        let err = questions_from_ron("(version: 1, questions: [ (id: \"x\" ").unwrap_err();
        let message = err.to_string();
        assert!(message.starts_with("1:"), "no position in {message:?}");
    }

    #[test]
    fn structural_errors_name_the_question_and_the_rule() {
        let cases: Vec<(Question, &str)> = vec![
            (question("Bad Id"), "lower-case"),
            (
                Question {
                    options: vec![option("a")],
                    ..question("one")
                },
                "has 1 options",
            ),
            (
                Question {
                    options: vec![option("a"), option("a")],
                    ..question("dup")
                },
                "two options are labelled `a`",
            ),
            (
                Question {
                    options: vec![
                        option("a"),
                        ReviewOption {
                            scripts: vec![],
                            ..option("b")
                        },
                    ],
                    ..question("sceneless")
                },
                "has no scene",
            ),
            (
                Question {
                    options: vec![
                        option("a"),
                        ReviewOption {
                            scripts: vec!["b.lisp".into()],
                            ..option("b")
                        },
                    ],
                    ..question("ext")
                },
                "not a .scm file",
            ),
        ];
        for (q, expected) in cases {
            let id = q.id.clone();
            let message = set(vec![q]).validate().unwrap_err().to_string();
            assert!(
                message.contains(&format!("`{id}`")) && message.contains(expected),
                "{message:?} should name `{id}` and say {expected:?}"
            );
        }
    }

    #[test]
    fn side_by_side_rejects_settings_overrides() {
        let mut q = question("sbs");
        q.side_by_side = true;
        assert!(q.validate().is_ok());
        q.options[0].settings.render = Some(RenderSettings::default());
        let message = q.validate().unwrap_err().to_string();
        assert!(message.contains("cannot be side_by_side"), "{message}");
    }

    #[test]
    fn duplicate_ids_and_wrong_versions_are_rejected() {
        let dup = set(vec![question("same"), question("same")]);
        assert!(
            dup.validate()
                .unwrap_err()
                .to_string()
                .contains("earlier question")
        );
        let mut old = set(vec![question("q")]);
        old.version = 0;
        assert!(matches!(old.validate(), Err(ReviewError::Version(0))));
    }

    #[test]
    fn answers_round_trip_and_an_empty_file_is_an_empty_set() {
        let mut answers = AnswerSet::default();
        answers.upsert(answer("q1", "a"));
        answers.upsert(Answer {
            choice: Choice::NoneOfThese("something else".into()),
            note: "multi\nline".into(),
            revision: None,
            ..answer("q2", "")
        });
        let text = answers_to_ron(&answers).unwrap();
        assert_eq!(answers_from_ron(&text).unwrap(), answers);
        assert_eq!(answers_from_ron("  \n").unwrap(), AnswerSet::default());
    }

    #[test]
    fn upsert_replaces_in_place_and_keeps_the_others() {
        let mut answers = AnswerSet::default();
        answers.upsert(answer("q1", "a"));
        answers.upsert(answer("q2", "a"));
        answers.upsert(answer("q3", "a"));
        answers.upsert(answer("q2", "b"));
        let ids: Vec<&str> = answers
            .answers
            .iter()
            .map(|a| a.question.as_str())
            .collect();
        assert_eq!(ids, ["q1", "q2", "q3"]);
        assert_eq!(
            answers.get("q2").unwrap().choice,
            Choice::Picked("b".into())
        );
        assert_eq!(
            answers.get("q1").unwrap().choice,
            Choice::Picked("a".into())
        );
    }

    #[test]
    fn archive_parses() {
        let archive = Archive {
            version: REVIEW_FORMAT_VERSION,
            entries: vec![ArchiveEntry {
                question: question("q1"),
                answer: answer("q1", "a"),
                ingested_as: "brief 4b-9".into(),
            }],
        };
        let text = options().to_string_pretty(&archive, pretty()).unwrap();
        assert_eq!(archive_from_ron(&text).unwrap(), archive);
    }

    #[test]
    fn overrides_replace_only_the_named_resources() {
        let mut env = EnvironmentRecord::default();
        env.sim.speed = 2.0;
        let over = SettingsOverride {
            render: Some(RenderSettings {
                bands: 2,
                ..RenderSettings::default()
            }),
            ..SettingsOverride::default()
        };
        over.apply_to(&mut env);
        assert_eq!(env.render.bands, 2);
        assert!((env.sim.speed - 2.0).abs() < f32::EPSILON);
    }
}
