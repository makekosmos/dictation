//! Imago buttons with the shared caption typography (manager-gpui's
//! button.rs convention): gpui-component's Medium label renders 16px and
//! ignores the page's inherited font size, so the label draws as a 12.5px
//! caption inside the existing Imago control — geometry, variants, focus
//! and behavior stay intact.
use ::gpui::{prelude::*, *};
use gpui_component::{button::Button as ComponentButton, Disableable};

pub use imago_gpui::button::ButtonKind;
pub const LABEL_SIZE: f32 = 12.5;
pub const LABEL_LINE_HEIGHT: f32 = 16.0;

#[derive(IntoElement)]
pub struct Button {
    inner: ComponentButton,
    label: Option<SharedString>,
}

impl Button {
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn on_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.inner = self.inner.on_click(listener);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }
}

impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        self.inner.style()
    }
}

fn caption(text: SharedString) -> Div {
    div()
        .min_w_0()
        .whitespace_nowrap()
        .overflow_hidden()
        .text_ellipsis()
        .text_size(px(LABEL_SIZE))
        .line_height(px(LABEL_LINE_HEIGHT))
        .child(text)
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut inner = self.inner;
        if let Some(name) = self.label.clone() {
            inner = inner.accessibility_label(name);
        }
        if let Some(text) = self.label {
            inner = inner.child(
                caption(text)
                    .id("dictation-button-caption")
                    .debug_selector(|| "dictation-button-caption".into()),
            );
        }
        inner
    }
}

pub fn button(id: impl Into<ElementId>, kind: ButtonKind) -> Button {
    Button {
        inner: imago_gpui::button::button(id, kind),
        label: None,
    }
}

/// Labelled button on the shared control: `primary` picks the accent kind,
/// ghost otherwise. Drop-in for the kit's `fields::btn`.
pub fn btn<T: 'static>(
    id: &'static str,
    label: &'static str,
    primary: bool,
    cx: &mut Context<T>,
    on_click: impl Fn(&mut T, &mut Context<T>) + 'static,
) -> Button {
    button(
        id,
        if primary {
            ButtonKind::Primary
        } else {
            ButtonKind::Ghost
        },
    )
    .label(label)
    .on_click(cx.listener(move |this, _, _, cx| {
        on_click(this, cx);
        cx.notify();
    }))
}

/// Button whose id is dynamic (per-row queue actions). Drop-in for the
/// kit's `fields::btn_id` — pass `cx.listener(...)`.
pub fn btn_id(
    id: &str,
    label: &'static str,
    listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    button(SharedString::from(id.to_string()), ButtonKind::Ghost)
        .label(label)
        .on_click(listener)
}
