//! Review mode (brief 4b-0): open questions load as live scenes, answers
//! land in the answers file, and a review session leaves no undo history
//! and no autosave behind.
//!
//! The seed-question guard loads **every** option of every question in
//! `docs/review/questions.ron` (alone, and side by side where allowed)
//! through the real loader and fails on any script error or missing file —
//! the review counterpart of `script_examples.rs`.

// Test helpers: a failed setup step should stop the test loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::harness::{body_count, headless_app_with, step};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::command::intent::UndoIntent;
use gradiance::domain::settings::{LightingSettings, RenderSettings};
use gradiance::persist::review::{
    ReviewAnswerRequest, ReviewSession, ReviewShowRequest, ReviewView, read_answers,
};
use gradiance::scene::review::{Answer, AnswerSet, Choice, answers_to_ron};
use std::path::{Path, PathBuf};

/// Frames a load may take before the test calls it stuck.
const MAX_LOAD_FRAMES: usize = 120;

fn questions_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/review/questions.ron")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("gradiance-it-review-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A headless app in review mode over the seed questions, writing answers to
/// `answers` (never to the repo's file). The start-up show of the first
/// question has finished when this returns.
fn review_app(answers: PathBuf) -> App {
    let mut session = ReviewSession::open(questions_file()).expect("seed questions parse");
    session.answers_path = answers;
    session.answers = read_answers(&session.answers_path).unwrap();
    let mut app = headless_app_with(move |app| {
        app.insert_resource(session);
    });
    wait_until_loaded(&mut app, "start-up show");
    app
}

fn session(app: &App) -> &ReviewSession {
    app.world().resource::<ReviewSession>()
}

fn wait_until_loaded(app: &mut App, what: &str) {
    app.update();
    for _ in 0..MAX_LOAD_FRAMES {
        if !session(app).busy {
            return;
        }
        app.update();
    }
    panic!("{what}: still loading after {MAX_LOAD_FRAMES} frames");
}

fn show(app: &mut App, question: usize, view: ReviewView) {
    app.world_mut()
        .write_message(ReviewShowRequest { question, view });
    wait_until_loaded(app, &format!("question {question} {view:?}"));
}

/// Bodies each seed option leaves in the world, by question id — so an
/// option whose merge silently failed (a no-op, not a script error) still
/// fails here.
fn expected_bodies(question: &str) -> &'static [usize] {
    match question {
        "startup-scene" => &[0, 1, 6],
        // floor + the merged flush pair + the merged T
        "fillet-bulge" => &[3, 3],
        "shading" => &[8, 8, 8],
        _ => &[],
    }
}

#[test]
fn every_seed_option_loads_without_errors_and_leaves_no_undo() {
    let dir = temp_dir("seed");
    let mut app = review_app(dir.join("answers.ron"));
    let questions = session(&app).questions.questions.clone();
    assert!(questions.len() >= 3, "seed at least three questions");
    for (q, question) in questions.iter().enumerate() {
        let expected = expected_bodies(&question.id);
        assert_eq!(
            expected.len(),
            question.options.len(),
            "add `{}` to expected_bodies",
            question.id
        );
        for (i, option) in question.options.iter().enumerate() {
            show(&mut app, q, ReviewView::Single(i));
            let s = session(&app);
            assert!(
                s.problems.is_empty(),
                "`{}` / `{}`: {:?}",
                question.id,
                option.label,
                s.problems
            );
            assert_eq!(s.shown, Some((q, ReviewView::Single(i))));
            assert_eq!(
                body_count(&mut app),
                expected[i],
                "`{}` / `{}` body count",
                question.id,
                option.label
            );
            assert_eq!(
                app.world().resource::<HistoryInfo>().undo_depth,
                0,
                "`{}` / `{}` left undo history",
                question.id,
                option.label
            );
        }
        if question.side_by_side {
            show(&mut app, q, ReviewView::SideBySide);
            let s = session(&app);
            assert!(
                s.problems.is_empty(),
                "`{}` side by side: {:?}",
                question.id,
                s.problems
            );
            let regions = s.regions.clone();
            assert_eq!(regions.len(), question.options.len());
            for pair in regions.windows(2) {
                assert!(
                    pair[0].max.x < pair[1].min.x,
                    "`{}` regions overlap: {regions:?}",
                    question.id
                );
            }
            assert_eq!(body_count(&mut app), expected.iter().sum::<usize>());
            assert_eq!(app.world().resource::<HistoryInfo>().undo_depth, 0);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn settings_overrides_apply_and_do_not_leak_into_the_next_option() {
    let dir = temp_dir("settings");
    let mut app = review_app(dir.join("answers.ron"));
    let q = session(&app).questions.position("shading").unwrap();
    show(&mut app, q, ReviewView::Single(1));
    assert!(!app.world().resource::<RenderSettings>().toon_enabled);
    show(&mut app, q, ReviewView::Single(2));
    assert!(app.world().resource::<RenderSettings>().toon_enabled);
    assert!(app.world().resource::<LightingSettings>().ssao);
    show(&mut app, q, ReviewView::Single(0));
    assert_eq!(
        *app.world().resource::<LightingSettings>(),
        LightingSettings::default()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn review_scenes_cannot_be_undone_and_are_never_autosaved() {
    let dir = temp_dir("isolation");
    let mut app = review_app(dir.join("answers.ron"));
    let q = session(&app).questions.position("startup-scene").unwrap();
    show(&mut app, q, ReviewView::Single(2));
    let bodies = body_count(&mut app);
    assert!(bodies > 0);
    // Undo has nothing to revert: the load is not a step.
    app.world_mut().write_message(UndoIntent);
    step(&mut app, 2);
    assert_eq!(body_count(&mut app), bodies);

    let autosave = dir.join("session.ron");
    app.insert_resource(gradiance::persist::AutosavePath(autosave.clone()));
    app.world_mut().write_message(AppExit::Success);
    app.update();
    assert!(!autosave.exists(), "review mode wrote the session autosave");

    // Control: the same world outside review mode does autosave, so the
    // assertion above is not vacuous.
    app.world_mut().remove_resource::<ReviewSession>();
    app.world_mut().write_message(AppExit::Success);
    app.update();
    assert!(autosave.exists(), "control: autosave should be written");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_answer_writes_one_entry_and_keeps_earlier_ones() {
    let dir = temp_dir("answers");
    let answers_path = dir.join("answers.ron");
    let earlier = Answer {
        question: "some-older-question".into(),
        title: "Older".into(),
        choice: Choice::NoneOfThese("my own idea".into()),
        note: "keep me".into(),
        answered_at: "2026-11-01T00:00:00Z".into(),
        revision: None,
    };
    let mut seeded = AnswerSet::default();
    seeded.upsert(earlier.clone());
    std::fs::write(&answers_path, answers_to_ron(&seeded).unwrap()).unwrap();

    let mut app = review_app(answers_path.clone());
    let q = session(&app).questions.position("startup-scene").unwrap();
    let label = session(&app).questions.questions[q].options[1]
        .label
        .clone();
    app.world_mut().write_message(ReviewAnswerRequest {
        question: q,
        choice: Choice::Picked(label.clone()),
        note: "a note\n".into(),
    });
    app.update();

    let on_disk = read_answers(&answers_path).unwrap();
    assert_eq!(on_disk.answers.len(), 2);
    assert_eq!(on_disk.answers[0], earlier, "an earlier answer changed");
    let new = &on_disk.answers[1];
    assert_eq!(new.question, "startup-scene");
    assert_eq!(new.choice, Choice::Picked(label));
    assert_eq!(new.note, "a note");
    assert!(new.answered_at.ends_with('Z') && new.answered_at.len() == 20);
    assert_eq!(
        session(&app).answers,
        on_disk,
        "session out of step with the file"
    );
    assert!(session(&app).problems.is_empty());

    // Answering again replaces that entry; it never adds a second one.
    app.world_mut().write_message(ReviewAnswerRequest {
        question: q,
        choice: Choice::NoneOfThese(String::new()),
        note: String::new(),
    });
    app.update();
    let on_disk = read_answers(&answers_path).unwrap();
    assert_eq!(on_disk.answers.len(), 2);
    assert_eq!(
        on_disk.answers[1].choice,
        Choice::NoneOfThese(String::new())
    );

    // An answer naming an option the question does not have is refused.
    app.world_mut().write_message(ReviewAnswerRequest {
        question: q,
        choice: Choice::Picked("no such option".into()),
        note: String::new(),
    });
    app.update();
    assert_eq!(read_answers(&answers_path).unwrap(), on_disk);
    assert!(!session(&app).problems.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
