//! The layer a held card is drawn on while it follows the pointer.
//!
//! It exists as a widget rather than as a positioned container because of what a message costs.
//! Every message rebuilds the view and lays the whole window out again; a pointer reports movement
//! many times a second, and a card that followed the pointer through messages therefore paid for a
//! full rebuild per report and fell behind the cursor it was supposed to be stuck to.
//!
//! A widget can redraw without any of that. This one keeps the pointer position in its own state,
//! asks for a redraw when it moves, and draws the card translated to wherever the pointer now is —
//! so a drag costs one drawn frame per movement and no rebuilds at all. It publishes exactly one
//! message in its lifetime: the moment the press travels far enough to become a drag, which is a
//! real change in what the interface shows and has to reach the application.

use iced::advanced::widget::{Tree, tree};
use iced::advanced::{
    Clipboard, Layout, Renderer as _, Shell, Widget, layout, mouse, overlay, renderer,
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

/// How far the pointer has to travel before a press becomes a drag rather than a click.
pub const THRESHOLD: f32 = 5.0;

/// Where the card hangs relative to the pointer, so the cursor stays visible over it.
const GRAB_OFFSET: Vector = Vector::new(18.0, 14.0);

pub struct DragLayer<'a, Message> {
    card: Element<'a, Message, iced::Theme, iced::Renderer>,
    /// Whether the press has already become a drag. Until it has, nothing is drawn.
    lifted: bool,
    /// Published once, when the pointer first travels past [`THRESHOLD`].
    on_lift: Message,
}

#[derive(Default)]
struct State {
    /// Where the pointer was first seen after the press, which the travel is measured from.
    origin: Option<Point>,
    position: Option<Point>,
}

impl<'a, Message> DragLayer<'a, Message> {
    pub fn new(
        card: impl Into<Element<'a, Message, iced::Theme, iced::Renderer>>,
        lifted: bool,
        on_lift: Message,
    ) -> Self {
        Self {
            card: card.into(),
            lifted,
            on_lift,
        }
    }
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for DragLayer<'_, Message>
where
    Message: Clone,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.card)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.card));
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // The card is laid out at the origin and moved at draw time, so following the pointer never
        // invalidates the layout.
        let card =
            self.card
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, &limits.loose());
        layout::Node::with_children(limits.max(), vec![card])
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let Event::Mouse(mouse::Event::CursorMoved { position }) = event else {
            return;
        };
        let state: &mut State = tree.state.downcast_mut();
        state.position = Some(*position);
        let origin = *state.origin.get_or_insert(*position);

        if self.lifted {
            // The card has moved, and nothing else on screen has: a redraw is the whole update.
            shell.request_redraw();
        } else if origin.distance(*position) > THRESHOLD {
            shell.publish(self.on_lift.clone());
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if !self.lifted {
            return;
        }
        let state: &State = tree.state.downcast_ref();
        let Some(position) = state.position else {
            return;
        };
        let card = layout.children().next().expect("the card's layout");
        let translation =
            position - Point::ORIGIN - GRAB_OFFSET - (card.position() - Point::ORIGIN);

        renderer.with_layer(*viewport, |renderer| {
            renderer.with_translation(translation, |renderer| {
                self.card.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    card,
                    cursor,
                    viewport,
                );
            });
        });
    }

    /// The layer never claims the cursor. `stack` hands the cursor to the layers under the topmost
    /// one that claims a mouse interaction, and claiming it here would take the pointer away from
    /// the category chips the card is being carried to.
    fn mouse_interaction(
        &self,
        _tree: &Tree,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        mouse::Interaction::None
    }

    fn overlay<'b>(
        &'b mut self,
        _tree: &'b mut Tree,
        _layout: Layout<'b>,
        _renderer: &iced::Renderer,
        _viewport: &Rectangle,
        _translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        None
    }
}

impl<'a, Message> From<DragLayer<'a, Message>> for Element<'a, Message, iced::Theme, iced::Renderer>
where
    Message: Clone + 'a,
{
    fn from(layer: DragLayer<'a, Message>) -> Self {
        Self::new(layer)
    }
}

#[cfg(test)]
mod tests {
    use iced::{Point, Size};

    use super::{DragLayer, THRESHOLD};

    fn moved(x: f32, y: f32) -> iced::Event {
        iced::Event::Mouse(iced::mouse::Event::CursorMoved {
            position: Point::new(x, y),
        })
    }

    fn simulator<'a>(lifted: bool) -> iced_test::Simulator<'a, (), iced::Theme> {
        iced_test::Simulator::with_size(
            iced_test::core::Settings::default(),
            Size::new(400.0, 300.0),
            DragLayer::new(iced::widget::text("card"), lifted, ()),
        )
    }

    /// The hand shakes by a point or two on every click. If that tore the card out of the grid,
    /// every launch would flicker, so the press stays a click until the pointer really travels.
    #[test]
    fn a_press_that_barely_moves_never_becomes_a_drag() {
        let mut simulator = simulator(false);

        let _ = simulator.simulate([moved(100.0, 100.0)]);
        let _ = simulator.simulate([moved(100.0 + THRESHOLD - 0.5, 100.0)]);

        assert!(
            simulator.into_messages().next().is_none(),
            "nothing was lifted"
        );
    }

    #[test]
    fn a_press_that_travels_lifts_the_card_once() {
        let mut simulator = simulator(false);

        let _ = simulator.simulate([moved(100.0, 100.0)]);
        let _ = simulator.simulate([moved(100.0 + THRESHOLD + 0.5, 100.0)]);

        assert_eq!(simulator.into_messages().count(), 1);
    }

    /// The card has to actually move with the pointer. Nothing else on screen changes during a
    /// drag, so a frame drawn after the pointer moved must differ from the one drawn before it.
    #[test]
    fn a_lifted_card_is_drawn_where_the_pointer_is() {
        let mut simulator = simulator(true);
        let theme = iced::Theme::Dark;
        let reference = std::env::temp_dir().join(format!(
            "despatch-drag-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("a clock")
                .as_nanos()
        ));

        let _ = simulator.simulate([moved(60.0, 60.0)]);
        let first = simulator.snapshot(&theme).expect("a drawn frame");
        assert!(
            first.matches_hash(&reference).expect("record the frame"),
            "the first frame is the reference"
        );

        let _ = simulator.simulate([moved(240.0, 200.0)]);
        let second = simulator.snapshot(&theme).expect("a drawn frame");
        let moved_card = !second.matches_hash(&reference).expect("compare the frames");

        let _ = std::fs::remove_file(reference.with_extension("sha256"));
        assert!(moved_card, "the card followed the pointer");
    }

    /// Once the card is following the pointer, movement costs a redraw and nothing else: a message
    /// would rebuild the whole window for every report the pointer makes.
    #[test]
    fn a_card_already_lifted_reports_nothing_as_it_moves() {
        let mut simulator = simulator(true);

        let _ = simulator.simulate([moved(100.0, 100.0)]);
        let _ = simulator.simulate([moved(300.0, 250.0)]);

        assert!(simulator.into_messages().next().is_none());
    }
}
