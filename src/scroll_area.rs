//! A scroll area with its scrollbar (in the theme's coral, shown when the content does not fit). The bar is a
//! sibling of the area, not a child: inside a scrolled element it would scroll away with the content.

use gpui_kit::component::scroll::{Scrollbar, ScrollbarAxis};
use gpui_kit::*;

/// Wrap `area` (an element that scrolls vertically) so its scrollbar sits over its right edge. `wrapper` is
/// the outer box, which takes the area's place in the layout (sizes, flex); the area fills it.
pub fn with_scrollbar(
    wrapper: Div,
    area: Stateful<Div>,
    handle: &ScrollHandle,
    id: &'static str,
) -> Div {
    wrapper.relative().child(area.track_scroll(handle)).child(
        div()
            .absolute()
            .inset_0()
            .debug_selector(move || format!("scrollbar-{id}"))
            .child(Scrollbar::new(handle).id(id).axis(ScrollbarAxis::Vertical)),
    )
}
