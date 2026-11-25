//! Keyboard focus for Iced 0.13 buttons, without changing their mouse behavior.
//! Register only enabled actions. Tab traversal uses widget::focus_next/previous.
use super::{message::Message, style as st};
use iced::advanced::{
    layout, overlay, renderer,
    widget::{
        operation::{Focusable, Operation},
        tree, Tree,
    },
    Clipboard, Layout, Shell, Widget,
};
use iced::{
    event,
    keyboard::{self, key::Named, Key},
    mouse, Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector,
};

/// Wrap a button (or tooltip) and supply its enabled keyboard action.
/// None leaves the child unchanged and excludes it from the focus traversal.
/// Callers behind a modal must also pass None to avoid activating hidden actions.
pub fn action<'a>(
    content: impl Into<Element<'a, Message>>,
    message: Option<Message>,
) -> Element<'a, Message> {
    let identity = message.as_ref().map(|m| format!("{m:?}"));
    Element::new(Action {
        content: content.into(),
        message,
        identity,
    })
}

struct Action<'a> {
    content: Element<'a, Message>,
    message: Option<Message>,
    identity: Option<String>,
}
#[derive(Default)]
struct State {
    identity: Option<String>,
    focused: bool,
    held: Option<Key>,
}
impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn focus(&mut self) {
        self.focused = true;
        self.held = None;
    }
    fn unfocus(&mut self) {
        self.focused = false;
        self.held = None;
    }
}
impl State {
    fn reconcile(&mut self, identity: &Option<String>) {
        if &self.identity != identity {
            self.unfocus();
            self.identity.clone_from(identity);
        }
    }
    /// Captured even on repeat/release, so global Space cannot toggle playback.
    fn key(&mut self, event: &keyboard::Event) -> (event::Status, bool) {
        if !self.focused {
            return (event::Status::Ignored, false);
        }
        match event {
            keyboard::Event::KeyPressed { key, modifiers, .. }
                if modifiers.is_empty()
                    && matches!(key, Key::Named(Named::Enter | Named::Space)) =>
            {
                let activate = self.held.is_none();
                self.held = Some(key.clone());
                (event::Status::Captured, activate)
            }
            keyboard::Event::KeyReleased { key, .. } if self.held.as_ref() == Some(key) => {
                self.held = None;
                (event::Status::Captured, false)
            }
            _ => (event::Status::Ignored, false),
        }
    }
}
impl Widget<Message, Theme, Renderer> for Action<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State {
            identity: self.identity.clone(),
            ..State::default()
        })
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.state.downcast_mut::<State>().reconcile(&self.identity);
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if self.message.is_some() {
            operation.focusable(tree.state.downcast_mut::<State>(), None);
        }
        self.content
            .as_widget()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) -> event::Status {
        let state = tree.state.downcast_mut::<State>();
        if let Some(message) = &self.message {
            if let Event::Keyboard(ref key) = event {
                let (status, activate) = state.key(key);
                if activate {
                    shell.publish(message.clone());
                }
                if status == event::Status::Captured {
                    return status;
                }
            }
            if matches!(
                event,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            ) {
                // Mouse clicks do not leave stale focus in a different action.
                state.unfocus();
            }
        }
        if matches!(event, Event::Window(iced::window::Event::Unfocused)) {
            state.unfocus();
        }
        self.content.as_widget_mut().on_event(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        )
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        if self.message.is_some() && tree.state.downcast_ref::<State>().focused {
            use iced::advanced::Renderer as _;
            // Inset ring needs no extra layout space and stays inside scroll clips.
            let bounds = layout.bounds().shrink(1.);
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: iced::Border {
                        color: st::readable_on(st::accent(), st::sidebar()),
                        width: 2.,
                        radius: st::radius::CONTROL.into(),
                    },
                    ..renderer::Quad::default()
                },
                iced::Color::TRANSPARENT,
            );
        }
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(&mut tree.children[0], layout, renderer, translation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pressed(key: Named) -> keyboard::Event {
        keyboard::Event::KeyPressed {
            key: Key::Named(key),
            modified_key: Key::Named(key),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: None,
        }
    }
    fn traverse(operation: &mut dyn Operation, states: &mut [State]) {
        for state in &mut *states {
            if state.identity.is_some() {
                operation.focusable(state, None);
            }
        }
        if let iced::advanced::widget::operation::Outcome::Chain(mut next) = operation.finish() {
            traverse(next.as_mut(), states);
        }
    }
    #[test]
    fn iced_tab_operations_skip_disabled_actions_and_move_in_both_directions() {
        use iced::advanced::widget::operation::focusable::{focus_next, focus_previous};
        let mut states: Vec<_> = [Some("nav"), None, Some("player")]
            .into_iter()
            .map(|id| State {
                identity: id.map(str::to_owned),
                ..State::default()
            })
            .collect();
        traverse(&mut focus_next::<()>(), &mut states);
        assert!(states[0].is_focused());
        assert!(!states[1].is_focused());
        traverse(&mut focus_next::<()>(), &mut states);
        assert!(!states[0].is_focused());
        assert!(states[2].is_focused());
        traverse(&mut focus_previous::<()>(), &mut states);
        assert!(states[0].is_focused());
        assert!(!states[2].is_focused());
        states[0].reconcile(&None);
        traverse(&mut focus_next::<()>(), &mut states);
        assert!(states[2].is_focused());
    }
    #[test]
    fn enter_and_space_activate_once_and_capture_repeats_and_release() {
        for key in [Named::Enter, Named::Space] {
            let mut state = State::default();
            state.focus();
            assert_eq!(state.key(&pressed(key)), (event::Status::Captured, true));
            assert_eq!(state.key(&pressed(key)), (event::Status::Captured, false));
            assert_eq!(
                state.key(&keyboard::Event::KeyReleased {
                    key: Key::Named(key),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::empty()
                }),
                (event::Status::Captured, false)
            );
            assert_eq!(state.key(&pressed(key)), (event::Status::Captured, true));
        }
    }
    #[test]
    fn unfocused_disabled_and_replaced_actions_cannot_activate() {
        let mut state = State::default();
        assert_eq!(
            state.key(&pressed(Named::Space)),
            (event::Status::Ignored, false)
        );
        state.reconcile(&Some("play".into()));
        state.focus();
        state.reconcile(&Some("stop".into()));
        assert!(!state.is_focused());
        state.focus();
        state.reconcile(&None);
        assert!(!state.is_focused());
        assert_eq!(
            state.key(&pressed(Named::Enter)),
            (event::Status::Ignored, false)
        );
    }
    #[test]
    fn tab_and_modified_activation_belong_to_the_input_router() {
        let mut state = State::default();
        state.focus();
        assert_eq!(
            state.key(&pressed(Named::Tab)),
            (event::Status::Ignored, false)
        );
        let mut event = pressed(Named::Space);
        if let keyboard::Event::KeyPressed { modifiers, .. } = &mut event {
            *modifiers = keyboard::Modifiers::CTRL;
        }
        assert_eq!(state.key(&event), (event::Status::Ignored, false));
    }
}
