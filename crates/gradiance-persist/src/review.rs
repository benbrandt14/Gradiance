//! Review mode's disk side and session state (brief 4b-0).
//!
//! `gradiance --review [questions.ron]` opens a [`ReviewSession`]: the
//! parsed questions file, the answers file beside it, and what is on
//! screen. The UI emits [`ReviewShowRequest`] (load a question's option, or
//! all of them side by side) and [`ReviewAnswerRequest`] (record a choice);
//! loading is the app shell's (`gradiance::review`, which can reach the
//! script layer), answering is here because it is a file write.
//!
//! Review mode is its own session: while a [`ReviewSession`] exists the exit
//! autosave is skipped (the owner's `.gradiance-session.ron` is never
//! overwritten by a review scene), and the loader discards undo history after
//! every load.
//!
//! The answers write is **atomic** (temp file + rename in the same directory)
//! and **re-reads the file first**, so an answer never drops an entry that is
//! on disk — including one the owner edited by hand while the app ran. A file
//! that no longer parses is left alone and the write fails loudly.

use bevy::prelude::*;
use gradiance_core::messages::drain;
use gradiance_scene::review::{
    Answer, AnswerSet, Choice, QuestionSet, ReviewError, answers_from_ron, answers_to_ron,
    questions_from_ron,
};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Default questions file, relative to the working directory (the repo root).
pub const DEFAULT_QUESTIONS_FILE: &str = "docs/review/questions.ron";

/// Name of the answers file, written beside the questions file.
pub const ANSWERS_FILE_NAME: &str = "answers.ron";

/// What a question shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewView {
    /// One option at a time (A/B flipping); the index into the options.
    Single(usize),
    /// Every option at once, laid out left to right in one world.
    SideBySide,
}

/// One option's patch of the world in a side-by-side view (world metres).
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewRegion {
    /// The option's label.
    pub label: String,
    /// Lower-left corner of the region.
    pub min: Vec2,
    /// Upper-right corner of the region.
    pub max: Vec2,
}

/// The open review session. Its presence *is* review mode.
#[derive(Resource, Debug, Clone)]
pub struct ReviewSession {
    /// The questions file.
    pub questions_path: PathBuf,
    /// The answers file the app writes.
    pub answers_path: PathBuf,
    /// The parsed, validated questions.
    pub questions: QuestionSet,
    /// The answers as last read or written.
    pub answers: AnswerSet,
    /// The question and view on screen, once something has loaded.
    pub shown: Option<(usize, ReviewView)>,
    /// Side-by-side regions of the shown view (empty for a single option).
    pub regions: Vec<ReviewRegion>,
    /// A load is in progress (the UI greys its buttons).
    pub busy: bool,
    /// Problems from the last load or answer, newest last; cleared when a
    /// new load starts.
    pub problems: Vec<String>,
    /// The last successful answer write, for the status line.
    pub last_saved: Option<String>,
    /// Logical pixels of the scene's right side covered by the review
    /// window (published by the UI; 0 headless). Side-by-side framing fits
    /// the regions into the part left uncovered.
    pub view_inset_right: f32,
}

impl ReviewSession {
    /// Opens `questions_path` (parsed and validated) and the answers file
    /// beside it (absent = no answers yet).
    pub fn open(questions_path: impl Into<PathBuf>) -> Result<Self, ReviewError> {
        let questions_path = questions_path.into();
        let text = read(&questions_path)?;
        let questions = questions_from_ron(&text).map_err(|e| in_file(&questions_path, e))?;
        let answers_path = questions_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(ANSWERS_FILE_NAME);
        let answers = read_answers(&answers_path)?;
        Ok(Self {
            questions_path,
            answers_path,
            questions,
            answers,
            shown: None,
            regions: Vec::new(),
            busy: false,
            problems: Vec::new(),
            last_saved: None,
            view_inset_right: 0.0,
        })
    }

    /// The directory option paths are relative to.
    pub fn base_dir(&self) -> PathBuf {
        self.questions_path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    }

    /// Index of the first question without an answer.
    pub fn first_unanswered(&self) -> Option<usize> {
        self.questions
            .questions
            .iter()
            .position(|q| self.answers.get(&q.id).is_none())
    }
}

/// Request: show question `question` in `view` (replaces the scene).
#[derive(Message, Debug, Clone, Copy)]
pub struct ReviewShowRequest {
    /// Index into the session's questions.
    pub question: usize,
    /// One option, or all side by side.
    pub view: ReviewView,
}

/// Request: record the owner's answer to question `question`.
#[derive(Message, Debug, Clone)]
pub struct ReviewAnswerRequest {
    /// Index into the session's questions.
    pub question: usize,
    /// The choice.
    pub choice: Choice,
    /// Free-text note (may be empty).
    pub note: String,
}

fn read(path: &Path) -> Result<String, ReviewError> {
    std::fs::read_to_string(path).map_err(|source| ReviewError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Prefixes a parse/validation error with the file it came from, so the
/// message reads `docs/review/questions.ron: 12:5: ...`.
fn in_file(path: &Path, error: ReviewError) -> ReviewError {
    match error {
        ReviewError::Io { .. } => error,
        other => ReviewError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, other.to_string()),
        },
    }
}

/// Reads an answers file; a missing file is an empty set.
pub fn read_answers(path: &Path) -> Result<AnswerSet, ReviewError> {
    match std::fs::read_to_string(path) {
        Ok(text) => answers_from_ron(&text).map_err(|e| in_file(path, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AnswerSet::default()),
        Err(source) => Err(ReviewError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Writes `text` to `path` atomically: a temp file in the same directory,
/// then a rename over the target, so a crash mid-write leaves either the old
/// file or the new one — never half of one.
pub fn write_atomically(path: &Path, text: &str) -> Result<(), ReviewError> {
    let io = |source| ReviewError::Io {
        path: path.to_path_buf(),
        source,
    };
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    std::fs::write(&tmp, text).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(io)
}

/// Records `answer` in the answers file at `path`: re-reads what is on disk,
/// replaces that question's entry (or appends), and writes atomically.
/// Returns the set now on disk. An unreadable file is not overwritten.
pub fn record_answer(path: &Path, answer: Answer) -> Result<AnswerSet, ReviewError> {
    let mut answers = read_answers(path)?;
    answers.upsert(answer);
    write_atomically(path, &answers_to_ron(&answers)?)?;
    Ok(answers)
}

/// `time` as an RFC 3339 UTC timestamp, second precision
/// (`2026-11-16T12:00:00Z`). Times before the epoch clamp to it.
pub fn utc_timestamp(time: SystemTime) -> String {
    let secs = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem / 60) % 60,
        rem % 60
    )
}

/// Days since 1970-01-01 → (year, month, day), proleptic Gregorian
/// (Howard Hinnant's `civil_from_days`, for non-negative day counts).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

/// The short git revision of the checkout containing `dir`, if git is
/// installed and `dir` is in a repository.
pub fn git_revision(dir: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let rev = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!rev.is_empty()).then_some(rev)
}

/// Drains [`ReviewAnswerRequest`]s into the answers file.
pub fn handle_review_answers(world: &mut World) {
    let requests: Vec<ReviewAnswerRequest> = drain(world);
    if requests.is_empty() {
        return;
    }
    let Some(mut session) = world.get_resource::<ReviewSession>().cloned() else {
        return;
    };
    for request in requests {
        let Some(question) = session.questions.questions.get(request.question) else {
            session
                .problems
                .push(format!("no question {} to answer", request.question));
            continue;
        };
        if let Choice::Picked(label) = &request.choice
            && question.option(label).is_none()
        {
            session
                .problems
                .push(format!("`{}` has no option `{label}`", question.id));
            continue;
        }
        let answer = Answer {
            question: question.id.clone(),
            title: question.title.clone(),
            choice: request.choice,
            note: request.note.trim_end().to_owned(),
            answered_at: utc_timestamp(SystemTime::now()),
            revision: git_revision(&session.base_dir()),
        };
        let id = answer.question.clone();
        match record_answer(&session.answers_path, answer) {
            Ok(answers) => {
                info!(question = %id, path = %session.answers_path.display(), "review answer saved");
                session.answers = answers;
                session.last_saved = Some(format!(
                    "saved `{id}` to {}",
                    session.answers_path.display()
                ));
            }
            Err(e) => {
                warn!(error = %e, "review answer not saved");
                session.problems.push(format!("answer not saved: {e}"));
            }
        }
    }
    world.insert_resource(session);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn answer(id: &str, label: &str) -> Answer {
        Answer {
            question: id.into(),
            title: String::new(),
            choice: Choice::Picked(label.into()),
            note: String::new(),
            answered_at: "2026-11-16T00:00:00Z".into(),
            revision: None,
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gradiance-review-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn timestamps_are_rfc3339_utc() {
        assert_eq!(
            utc_timestamp(SystemTime::UNIX_EPOCH),
            "1970-01-01T00:00:00Z"
        );
        // 2026-11-16T13:14:15Z
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_794_834_855);
        assert_eq!(utc_timestamp(t), "2026-11-16T13:14:15Z");
        // A leap day.
        let t = SystemTime::UNIX_EPOCH + Duration::from_hours(264_384);
        assert_eq!(utc_timestamp(t), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn record_answer_keeps_earlier_answers_and_leaves_no_temp_file() {
        let dir = temp_dir("record");
        let path = dir.join(ANSWERS_FILE_NAME);
        record_answer(&path, answer("q1", "a")).unwrap();
        record_answer(&path, answer("q2", "b")).unwrap();
        let on_disk = read_answers(&path).unwrap();
        assert_eq!(on_disk.answers.len(), 2);
        assert_eq!(
            on_disk.get("q1").unwrap().choice,
            Choice::Picked("a".into())
        );
        // Re-answering replaces, never duplicates.
        record_answer(&path, answer("q1", "c")).unwrap();
        let on_disk = read_answers(&path).unwrap();
        assert_eq!(on_disk.answers.len(), 2);
        assert_eq!(
            on_disk.get("q1").unwrap().choice,
            Choice::Picked("c".into())
        );
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name() != ANSWERS_FILE_NAME)
            .collect();
        assert!(leftovers.is_empty(), "temp file left behind: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_answers_file_is_not_overwritten() {
        let dir = temp_dir("corrupt");
        let path = dir.join(ANSWERS_FILE_NAME);
        std::fs::write(&path, "(version: 1, answers: [ oops").unwrap();
        let err = record_answer(&path, answer("q1", "a")).unwrap_err();
        assert!(err.to_string().contains(ANSWERS_FILE_NAME), "{err}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "(version: 1, answers: [ oops"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_a_bad_questions_file_names_the_file() {
        let dir = temp_dir("open");
        let path = dir.join("questions.ron");
        std::fs::write(&path, "(version: 1, questions: [(id: \"Bad\")])").unwrap();
        let err = ReviewSession::open(&path).unwrap_err().to_string();
        assert!(err.contains("questions.ron"), "{err}");
        let missing = ReviewSession::open(dir.join("nope.ron"))
            .unwrap_err()
            .to_string();
        assert!(missing.contains("nope.ron"), "{missing}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
