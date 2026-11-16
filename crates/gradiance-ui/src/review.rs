//! The **Review** window: open product questions as live scenes (brief
//! 4b-0, `gradiance --review`).
//!
//! A thin consumer, like every panel: it reads `persist::review`'s
//! [`ReviewSession`] and emits two requests — show a question's option (or
//! all of them side by side) and record an answer. Loading is the app
//! shell's (`gradiance::review`), the answers write is `persist`'s. The
//! window's own state is the answer being drafted (`EditorState` seam).
//!
//! It is a floating window rather than a dock pane: a review is a session you
//! open, work through and close, not a view of the model you keep beside
//! your work (`docs/ui-design.md` section 3). Side-by-side regions are named
//! in the viewport by floating tags over each option's patch of the world.

use crate::widgets;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_persist::review::{
    ReviewAnswerRequest, ReviewSession, ReviewShowRequest, ReviewView,
};
use gradiance_scene::review::{Choice, Question};

/// Whether the Review window is showing, and the answer being drafted.
#[derive(Resource, Default, Debug)]
pub struct ReviewWindow {
    /// Window visibility.
    pub open: bool,
    /// The draft answer's question; a different question resets the draft.
    draft_for: Option<usize>,
    /// The drafted choice, once one is made.
    pick: Option<Pick>,
    /// "None of these" free text.
    other: String,
    /// The note.
    note: String,
}

crate::impl_panel_toggle!(ReviewWindow, open);

/// A drafted choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// The option at this index.
    Option(usize),
    /// None of these.
    Other,
}

/// Opens the window at start-up when the app was launched in review mode.
pub fn open_in_review_mode(session: Option<Res<ReviewSession>>, mut window: ResMut<ReviewWindow>) {
    if session.is_some() {
        window.open = true;
    }
}

/// The letter an option is shown with: A, B, C, D.
pub fn option_letter(index: usize) -> char {
    u8::try_from(index)
        .ok()
        .and_then(|i| b'A'.checked_add(i))
        .map_or('?', char::from)
}

/// The draft for `question`, seeded from its saved answer: (pick, other
/// text, note). Pure, so the seeding rule is testable.
pub fn draft_from_answer(
    question: &Question,
    session: &ReviewSession,
) -> (Option<Pick>, String, String) {
    let Some(answer) = session.answers.get(&question.id) else {
        return (None, String::new(), String::new());
    };
    match &answer.choice {
        Choice::Picked(label) => (
            question
                .options
                .iter()
                .position(|o| &o.label == label)
                .map(Pick::Option),
            String::new(),
            answer.note.clone(),
        ),
        Choice::NoneOfThese(text) => (Some(Pick::Other), text.clone(), answer.note.clone()),
    }
}

/// Renders the Review window and the side-by-side region tags.
pub fn review_window(
    mut contexts: EguiContexts,
    mut window: ResMut<ReviewWindow>,
    session: Option<Res<ReviewSession>>,
    mut show: MessageWriter<ReviewShowRequest>,
    mut answer: MessageWriter<ReviewAnswerRequest>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if let Some(session) = &session
        && let Ok((camera, pose)) = cameras.single()
    {
        region_tags(ctx, session, camera, pose);
    }
    if !window.open {
        return Ok(());
    }
    let mut open = true;
    egui::Window::new("Review")
        .open(&mut open)
        .default_pos(egui::pos2(70.0, 60.0))
        .default_width(360.0)
        .default_height(560.0)
        .resizable(true)
        .vscroll(false)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match &session {
                    Some(session) => {
                        review_body(ui, session, &mut window, &mut show, &mut answer);
                    }
                    None => {
                        widgets::empty_state(
                            ui,
                            "Not in review mode. Start the app with \
                             `gradiance --review` to answer the open questions \
                             in docs/review/questions.ron.",
                        );
                    }
                });
        });
    if !open {
        window.open = false;
    }
    Ok(())
}

fn review_body(
    ui: &mut egui::Ui,
    session: &ReviewSession,
    window: &mut ReviewWindow,
    show: &mut MessageWriter<ReviewShowRequest>,
    answer: &mut MessageWriter<ReviewAnswerRequest>,
) {
    let questions = &session.questions.questions;
    let answered = questions
        .iter()
        .filter(|q| session.answers.get(&q.id).is_some())
        .count();
    widgets::hint(
        ui,
        &format!(
            "{answered} of {} answered - answers go to {}",
            questions.len(),
            session.answers_path.display()
        ),
    );
    if questions.is_empty() {
        widgets::empty_state(
            ui,
            "No open questions. Add one to the questions file \
             (docs/orchestration.md, Review mode).",
        );
        return;
    }

    widgets::section_header(ui, "Questions");
    let current = session.shown.map(|(q, _)| q);
    for (i, question) in questions.iter().enumerate() {
        let status = session.answers.get(&question.id).map_or_else(
            || "open".to_owned(),
            |a| format!("answered: {}", a.choice.summary()),
        );
        let selected = current == Some(i);
        let response = ui
            .add_enabled(
                !session.busy,
                egui::Button::selectable(selected, &question.title),
            )
            .on_hover_text(&question.id);
        widgets::hint(ui, &status);
        if response.clicked() && !selected {
            show.write(ReviewShowRequest {
                question: i,
                view: ReviewView::Single(0),
            });
        }
    }

    let Some((q, view)) = session.shown else {
        ui.separator();
        widgets::hint(ui, "Loading...");
        return;
    };
    let Some(question) = questions.get(q) else {
        return;
    };
    if window.draft_for != Some(q) {
        let (pick, other, note) = draft_from_answer(question, session);
        window.draft_for = Some(q);
        window.pick = pick;
        window.other = other;
        window.note = note;
    }

    question_view(ui, session, question, q, view, show);
    answer_form(ui, question, q, window, answer);
    if session.busy {
        widgets::hint(ui, "Loading...");
    }
    if let Some(saved) = &session.last_saved {
        widgets::hint(ui, saved);
    }
    for problem in &session.problems {
        ui.colored_label(ui.visuals().error_fg_color, problem);
    }
}

/// The shown question: its description and the option switcher.
fn question_view(
    ui: &mut egui::Ui,
    session: &ReviewSession,
    question: &Question,
    q: usize,
    view: ReviewView,
    show: &mut MessageWriter<ReviewShowRequest>,
) {
    ui.separator();
    ui.heading(&question.title);
    if !question.source.is_empty() {
        widgets::hint(ui, &format!("from: {}", question.source));
    }
    ui.label(&question.description);

    ui.separator();
    widgets::section_header(ui, "Show");
    ui.horizontal_wrapped(|ui| {
        for (i, option) in question.options.iter().enumerate() {
            let on = view == ReviewView::Single(i);
            let text = format!("{}  {}", option_letter(i), option.label);
            if ui
                .add_enabled(!session.busy, egui::Button::selectable(on, text))
                .on_hover_text(&option.caption)
                .clicked()
                && !on
            {
                show.write(ReviewShowRequest {
                    question: q,
                    view: ReviewView::Single(i),
                });
            }
        }
        if question.side_by_side {
            let on = view == ReviewView::SideBySide;
            if ui
                .add_enabled(!session.busy, egui::Button::selectable(on, "Side by side"))
                .on_hover_text("every option at once, left to right, in one world")
                .clicked()
                && !on
            {
                show.write(ReviewShowRequest {
                    question: q,
                    view: ReviewView::SideBySide,
                });
            }
        }
    });
    if !question.side_by_side {
        widgets::hint(
            ui,
            "One at a time: these options differ in settings (or would \
             collide), so they cannot share one world.",
        );
    }
    match view {
        ReviewView::Single(i) => {
            if let Some(option) = question.options.get(i) {
                ui.label(
                    egui::RichText::new(format!("{}  {}", option_letter(i), option.label)).strong(),
                );
                ui.label(&option.caption);
            }
        }
        ReviewView::SideBySide => {
            for (i, option) in question.options.iter().enumerate() {
                ui.label(
                    egui::RichText::new(format!("{}  {}", option_letter(i), option.label)).strong(),
                );
                ui.label(&option.caption);
            }
        }
    }
    widgets::hint(
        ui,
        "The scene is live: play, pause, drag bodies, change settings. \
         Nothing here is autosaved or kept in undo history.",
    );
}

/// The answer form: pick, free text, note, save.
fn answer_form(
    ui: &mut egui::Ui,
    question: &Question,
    q: usize,
    window: &mut ReviewWindow,
    answer: &mut MessageWriter<ReviewAnswerRequest>,
) {
    ui.separator();
    widgets::section_header(ui, "Your answer");
    for (i, option) in question.options.iter().enumerate() {
        ui.radio_value(
            &mut window.pick,
            Some(Pick::Option(i)),
            format!("{}  {}", option_letter(i), option.label),
        );
    }
    ui.radio_value(&mut window.pick, Some(Pick::Other), "None of these");
    if window.pick == Some(Pick::Other) {
        ui.add(
            egui::TextEdit::singleline(&mut window.other)
                .hint_text("what you want instead")
                .desired_width(f32::INFINITY),
        );
    }
    ui.label("Note (optional)");
    ui.add(
        egui::TextEdit::multiline(&mut window.note)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    let choice = match window.pick {
        Some(Pick::Option(i)) => question
            .options
            .get(i)
            .map(|o| Choice::Picked(o.label.clone())),
        Some(Pick::Other) => Some(Choice::NoneOfThese(window.other.trim().to_owned())),
        None => None,
    };
    if ui
        .add_enabled(choice.is_some(), egui::Button::new("Save answer"))
        .on_hover_text("write this answer to the answers file (replaces an earlier one)")
        .clicked()
        && let Some(choice) = choice
    {
        answer.write(ReviewAnswerRequest {
            question: q,
            choice,
            note: window.note.clone(),
        });
    }
}

/// Names each side-by-side region in the viewport: a tag above its patch.
fn region_tags(
    ctx: &egui::Context,
    session: &ReviewSession,
    camera: &Camera,
    pose: &GlobalTransform,
) {
    for (i, region) in session.regions.iter().enumerate() {
        let top = Vec3::new(f32::midpoint(region.min.x, region.max.x), region.max.y, 0.0);
        let Ok(screen) = camera.world_to_viewport(pose, top) else {
            continue;
        };
        egui::Area::new(egui::Id::new(("review-region", i)))
            .fixed_pos(egui::pos2(screen.x, screen.y - 12.0))
            .pivot(egui::Align2::CENTER_BOTTOM)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_black_alpha(170))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(format!("{}  {}", option_letter(i), region.label))
                                .color(egui::Color32::WHITE)
                                .strong(),
                        );
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_are_lettered_a_to_d() {
        let letters: String = (0..4).map(option_letter).collect();
        assert_eq!(letters, "ABCD");
    }
}
