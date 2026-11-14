// Text-field layout and GPUI input plumbing adapted from gpui 0.2.2's input example.
// Copyright Zed Industries, Inc. Licensed under Apache-2.0.
// https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/input.rs
// Console Link additions: bounded single-line input, safe IME ranges, scrolling, and events.

use std::ops::Range;

use super::theme;

use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, KeyBinding,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, actions, div,
    fill, point, prelude::*, px, relative, rgb, rgba, size,
};
use unicode_segmentation::*;

actions!(
    console_text_field,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
    ]
);

pub struct TextField {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    max_chars: usize,
    scroll_x: Pixels,
}

#[derive(Clone, Debug)]
pub struct TextChanged(pub String);
impl EventEmitter<TextChanged> for TextField {}

impl TextField {
    pub fn new(
        placeholder: impl Into<SharedString>,
        max_chars: usize,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            max_chars,
            scroll_x: px(0.),
        }
    }
    pub fn value(&self) -> &str {
        &self.content
    }
    /// Synchronize application state without emitting a user-edit event.
    pub fn set_value(&mut self, value: impl Into<SharedString>, cx: &mut Context<Self>) {
        let value = value.into();
        let value = single_line(&value, self.max_chars);
        if self.content.as_ref() == value {
            return;
        }
        self.content = value.into();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        cx.notify();
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.is_selecting = true;

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting && event.pressed_button == Some(MouseButton::Left) {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.is_selecting = false;
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace("\n", " "), window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }
    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.closest_index_for_x(position.x - bounds.left() + self.scroll_x)
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        utf16_to_utf8(&self.content, offset)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        {
            let start = self.offset_from_utf16(range_utf16.start);
            let end = self.offset_from_utf16(range_utf16.end).max(start);
            start..end
        }
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let inserted = replace_limited(&mut self.content, range.clone(), new_text, self.max_chars);
        let end = range.start + inserted.len();
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range = None;
        cx.emit(TextChanged(self.content.to_string()));
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let inserted = replace_limited(&mut self.content, range.clone(), new_text, self.max_chars);
        self.marked_range =
            (!inserted.is_empty()).then_some(range.start..range.start + inserted.len());
        self.selected_range =
            composition_selection(&inserted, range.start, new_selected_range_utf16);
        self.selection_reversed = false;
        cx.emit(TextChanged(self.content.to_string()));
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start) - self.scroll_x,
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end) - self.scroll_x,
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let local = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let utf8_index = last_layout.index_for_x(local.x + self.scroll_x)?;
        Some(self.offset_to_utf16(utf8_index))
    }
}

struct TextElement {
    input: Entity<TextField>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), rgb(0x858585).into())
        } else {
            (content, style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let cursor_pos = line.x_for_index(cursor);
        let mut scroll_x = input.scroll_x;
        let visible_width = (bounds.size.width - px(3.)).max(px(1.));
        if cursor_pos < scroll_x {
            scroll_x = cursor_pos;
        }
        if cursor_pos > scroll_x + visible_width {
            scroll_x = cursor_pos - visible_width;
        }
        scroll_x = scroll_x
            .min((line.width - visible_width).max(px(0.)))
            .max(px(0.));
        self.input.update(cx, |input, _| input.scroll_x = scroll_x);
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos - scroll_x, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    rgb(0xffffff),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected_range.start) - scroll_x,
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected_range.end) - scroll_x,
                            bounds.bottom(),
                        ),
                    ),
                    rgba(0xffffff30),
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection)
        }
        let line = prepaint.line.take().unwrap();
        let origin = point(bounds.left() - self.input.read(cx).scroll_x, bounds.top());
        let _ = line.paint(origin, window.line_height(), window, cx);

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for TextField {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .key_context("ConsoleTextField")
            .track_focus(&self.focus_handle(cx))
            .tab_stop(true)
            .focus(|s| s.border_color(rgb(theme::TEXT)))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .min_w(px(0.))
            .rounded_md()
            .border_1()
            .border_color(rgb(theme::CONTROL_BORDER))
            .bg(rgb(theme::BACKGROUND))
            .text_color(rgb(theme::TEXT))
            .line_height(px(22.))
            .text_size(px(14.))
            .child(
                div()
                    .h(px(38.))
                    .w_full()
                    .px(px(10.))
                    .py(px(7.))
                    .overflow_hidden()
                    .child(TextElement { input: cx.entity() }),
            )
    }
}

impl Focusable for TextField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Text actions stay scoped to an editor; gameplay M/+/- bindings do not run while typing.
pub fn install(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("ConsoleTextField")),
        KeyBinding::new("delete", Delete, Some("ConsoleTextField")),
        KeyBinding::new("left", Left, Some("ConsoleTextField")),
        KeyBinding::new("right", Right, Some("ConsoleTextField")),
        KeyBinding::new("shift-left", SelectLeft, Some("ConsoleTextField")),
        KeyBinding::new("shift-right", SelectRight, Some("ConsoleTextField")),
        KeyBinding::new("cmd-a", SelectAll, Some("ConsoleTextField")),
        KeyBinding::new("cmd-v", Paste, Some("ConsoleTextField")),
        KeyBinding::new("cmd-c", Copy, Some("ConsoleTextField")),
        KeyBinding::new("cmd-x", Cut, Some("ConsoleTextField")),
        KeyBinding::new("ctrl-a", SelectAll, Some("ConsoleTextField")),
        KeyBinding::new("ctrl-v", Paste, Some("ConsoleTextField")),
        KeyBinding::new("ctrl-c", Copy, Some("ConsoleTextField")),
        KeyBinding::new("ctrl-x", Cut, Some("ConsoleTextField")),
        KeyBinding::new("home", Home, Some("ConsoleTextField")),
        KeyBinding::new("end", End, Some("ConsoleTextField")),
        KeyBinding::new(
            "ctrl-cmd-space",
            ShowCharacterPalette,
            Some("ConsoleTextField"),
        ),
    ]);
}

fn single_line(text: &str, max_chars: usize) -> String {
    text.chars()
        .map(|c| {
            if matches!(c, '\n' | '\r' | '\t' | '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                c
            }
        })
        .filter(|c| !c.is_control())
        .take(max_chars)
        .collect()
}

fn utf16_to_utf8(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (index, c) in text.char_indices() {
        if units + c.len_utf16() > offset {
            return index;
        }
        units += c.len_utf16();
    }
    text.len()
}

fn composition_selection(
    inserted: &str,
    start: usize,
    selected: Option<Range<usize>>,
) -> Range<usize> {
    selected
        .map(|selection| {
            let low = utf16_to_utf8(inserted, selection.start);
            let high = utf16_to_utf8(inserted, selection.end).max(low);
            start + low..start + high
        })
        .unwrap_or_else(|| {
            let end = start + inserted.len();
            end..end
        })
}

fn replace_limited(
    content: &mut SharedString,
    range: Range<usize>,
    text: &str,
    limit: usize,
) -> String {
    let retained = content[..range.start].chars().count() + content[range.end..].chars().count();
    let inserted = single_line(text, limit.saturating_sub(retained));
    *content = format!(
        "{}{}{}",
        &content[..range.start],
        inserted,
        &content[range.end..]
    )
    .into();
    inserted
}

/// GPUI SVGs need an explicit paint color; inherited text color does not paint them.
pub fn icon(path: &'static str, size: f32, color: u32) -> gpui::Svg {
    gpui::svg()
        .path(path)
        .size(px(size))
        .flex_shrink_0()
        .text_color(rgb(color))
}

/// Shared selector: its options float above the page instead of changing card height.
/// Explicit widths keep GPUI's intrinsic text measurement out of the flex shrink path.
pub fn select(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    open: bool,
    options: Vec<(String, String, bool)>,
    on_toggle: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    on_select: impl Fn(String, &mut Window, &mut App) + 'static,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    Select {
        id: id.into(),
        label: label.into(),
        open,
        options,
        on_toggle: std::rc::Rc::new(on_toggle),
        on_select: std::rc::Rc::new(on_select),
        on_dismiss: std::rc::Rc::new(on_dismiss),
    }
    .into_any_element()
}

type SelectToggle = std::rc::Rc<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>;
type SelectChoice = std::rc::Rc<dyn Fn(String, &mut Window, &mut App)>;
type SelectDismiss = std::rc::Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(gpui::IntoElement)]
struct Select {
    id: ElementId,
    label: SharedString,
    open: bool,
    options: Vec<(String, String, bool)>,
    on_toggle: SelectToggle,
    on_select: SelectChoice,
    on_dismiss: SelectDismiss,
}

struct SelectState {
    trigger: FocusHandle,
    options: Vec<(String, FocusHandle)>,
    scroll: gpui::ScrollHandle,
}

impl RenderOnce for Select {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        use std::{cell::Cell, rc::Rc};
        let state =
            window.use_keyed_state((self.id.clone(), "select-state"), cx, |_, cx| SelectState {
                trigger: cx.focus_handle(),
                options: Vec::new(),
                scroll: gpui::ScrollHandle::new(),
            });
        // Handles belong to option values, not the deferred tree's global tab order.
        // Retaining them also keeps focus stable when another UI element rerenders.
        state.update(cx, |state, cx| {
            let previous = std::mem::take(&mut state.options);
            state.options = self
                .options
                .iter()
                .map(|(value, _, _)| {
                    let focus = previous
                        .iter()
                        .find(|(key, _)| key == value)
                        .map(|(_, focus)| focus.clone())
                        .unwrap_or_else(|| cx.focus_handle());
                    (value.clone(), focus)
                })
                .collect();
        });
        let trigger_focus = state.read(cx).trigger.clone();
        let option_focus: Rc<Vec<_>> = Rc::new(
            state
                .read(cx)
                .options
                .iter()
                .map(|(_, handle)| handle.clone())
                .collect(),
        );
        let scroll = state.read(cx).scroll.clone();
        let selected_index = self
            .options
            .iter()
            .position(|(_, _, selected)| *selected)
            .unwrap_or(0);
        let trigger_bounds = Rc::new(Cell::new(Bounds::<Pixels>::default()));
        let open = self.open;
        let click_toggle = self.on_toggle.clone();
        let key_toggle = self.on_toggle.clone();
        let click_trigger = trigger_focus.clone();
        let click_options = option_focus.clone();
        let click_scroll = scroll.clone();
        let key_options = option_focus.clone();
        let key_scroll = scroll.clone();
        let key_bounds = trigger_bounds.clone();
        let dismiss = self.on_dismiss.clone();
        let escape_focus = trigger_focus.clone();
        let trigger = div()
            .id(self.id.clone())
            .track_focus(&trigger_focus)
            .tab_stop(true)
            .w_full()
            .min_w_0()
            .min_h(px(42.))
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(rgb(if open {
                theme::SECONDARY
            } else {
                theme::BORDER
            }))
            .bg(rgb(theme::ELEVATED))
            .text_color(rgb(theme::TEXT))
            .text_size(px(14.))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(theme::HOVER)))
            .focus(|s| s.border_color(rgb(theme::TEXT)))
            .on_click(move |event, window, cx| {
                click_toggle(event, window, cx);
                if !open && let Some(focus) = click_options.get(selected_index) {
                    click_scroll.scroll_to_item(selected_index);
                    focus.focus(window);
                } else {
                    click_trigger.focus(window);
                }
                cx.stop_propagation();
            })
            .on_key_down(move |event, window, cx| {
                let index = match event.keystroke.key.as_str() {
                    "down" => selected_index,
                    "up" => {
                        if open {
                            selected_index
                        } else {
                            key_options.len().saturating_sub(1)
                        }
                    }
                    _ => return,
                };
                if !open {
                    key_toggle(
                        &gpui::ClickEvent::Keyboard(gpui::KeyboardClickEvent {
                            button: gpui::KeyboardButton::Enter,
                            bounds: key_bounds.get(),
                        }),
                        window,
                        cx,
                    );
                }
                if let Some(focus) = key_options.get(index) {
                    key_scroll.scroll_to_item(index);
                    focus.focus(window);
                }
                cx.stop_propagation();
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .w_full()
                    .line_height(relative(1.4))
                    .child(self.label),
            )
            .child(icon("ui/chevron-down.svg", 16., theme::SECONDARY));
        div()
            .id((self.id.clone(), "selector"))
            .relative()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .when(open, |root| {
                root.on_action(move |_: &super::Escape, window, cx| {
                    dismiss(window, cx);
                    escape_focus.focus(window);
                    cx.stop_propagation();
                })
            })
            .child(trigger)
            .child(
                gpui::canvas(
                    move |bounds, window, cx| {
                        trigger_bounds.set(bounds);
                        if !open {
                            return;
                        }
                        let viewport = window.viewport_size();
                        let limit = select_menu_bounds(bounds, viewport, px(280.));
                        let dismiss = self.on_dismiss.clone();
                        let mut menu = div()
                            .id((self.id.clone(), "menu"))
                            .w(limit.size.width)
                            .max_h(limit.size.height)
                            .overflow_y_scroll()
                            .track_scroll(&scroll)
                            .flex()
                            .flex_col()
                            .gap_1()
                            .p_1()
                            .rounded_xl()
                            .border_1()
                            .border_color(rgb(theme::BORDER))
                            .bg(rgb(theme::ELEVATED))
                            .shadow_lg()
                            .on_mouse_down_out(move |event, window, cx| {
                                if !bounds.contains(&event.position) {
                                    dismiss(window, cx);
                                }
                            });
                        for (index, (value, label, selected)) in
                            self.options.into_iter().enumerate()
                        {
                            let choose = self.on_select.clone();
                            let clicked_trigger = trigger_focus.clone();
                            let keyed_trigger = trigger_focus.clone();
                            let handles = option_focus.clone();
                            let row_scroll = scroll.clone();
                            let dismiss = self.on_dismiss.clone();
                            menu = menu.child(
                                div()
                                    .id((self.id.clone(), SharedString::from(value.clone())))
                                    .track_focus(&option_focus[index])
                                    .tab_stop(false)
                                    .w_full()
                                    .min_w_0()
                                    .flex_shrink_0()
                                    .min_h(px(40.))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(gpui::transparent_black())
                                    .bg(rgb(if selected {
                                        theme::HOVER
                                    } else {
                                        theme::ELEVATED
                                    }))
                                    .hover(|s| s.bg(rgb(theme::HOVER)))
                                    .focus(|s| s.border_color(rgb(theme::TEXT)))
                                    .cursor_pointer()
                                    .text_size(px(14.))
                                    .line_height(relative(1.4))
                                    .text_color(rgb(theme::TEXT))
                                    .on_click(move |_, window, cx| {
                                        choose(value.clone(), window, cx);
                                        clicked_trigger.focus(window);
                                        cx.stop_propagation();
                                    })
                                    .on_key_down(move |event, window, cx| {
                                        let next = match event.keystroke.key.as_str() {
                                            "down" => Some((index + 1).min(handles.len() - 1)),
                                            "up" => Some(index.saturating_sub(1)),
                                            "home" => Some(0),
                                            "end" => Some(handles.len() - 1),
                                            "tab" => {
                                                dismiss(window, cx);
                                                keyed_trigger.focus(window);
                                                if event.keystroke.modifiers.shift {
                                                    window.focus_prev();
                                                } else {
                                                    window.focus_next();
                                                }
                                                None
                                            }
                                            _ => return,
                                        };
                                        if let Some(next) = next {
                                            row_scroll.scroll_to_item(next);
                                            handles[next].focus(window);
                                        }
                                        cx.stop_propagation();
                                    })
                                    .child(div().flex_1().min_w_0().w_full().child(label))
                                    .child(
                                        div().size(px(18.)).flex_shrink_0().when(selected, |s| {
                                            s.child(icon("ui/check.svg", 18., theme::TEXT))
                                        }),
                                    ),
                            );
                        }
                        let mut menu = menu.into_any_element();
                        let measured = menu.layout_as_root(
                            size(
                                gpui::AvailableSpace::Definite(limit.size.width),
                                gpui::AvailableSpace::MaxContent,
                            ),
                            window,
                            cx,
                        );
                        let placement = select_menu_bounds(bounds, viewport, measured.height);
                        // defer_draw uses an absolute window offset. No local anchor offset is added.
                        window.defer_draw(menu, placement.origin, 2);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0()
                .size_full(),
            )
    }
}

/// The popup tracks the full trigger, including wrapped labels, and uses whichever
/// vertical side can contain it. Only a viewport narrower than the trigger limits width.
fn select_menu_bounds(
    trigger: Bounds<Pixels>,
    viewport: gpui::Size<Pixels>,
    natural_height: Pixels,
) -> Bounds<Pixels> {
    let margin = px(12.);
    let gap = px(4.);
    let width = trigger
        .size
        .width
        .min((viewport.width - margin * 2.).max(px(0.)));
    let below = (viewport.height - margin - trigger.bottom() - gap).max(px(0.));
    let above = (trigger.top() - gap - margin).max(px(0.));
    let down = natural_height <= below || below >= above;
    let height = natural_height
        .min(if down { below } else { above })
        .max(px(0.));
    let left = trigger
        .left()
        .max(margin)
        .min((viewport.width - margin - width).max(margin));
    let top = if down {
        trigger.bottom() + gap
    } else {
        trigger.top() - gap - height
    };
    Bounds::new(point(left, top), size(width, height))
}

pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    primary: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id.into())
        .focusable()
        .tab_stop(true)
        .flex()
        .min_w_0()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .px_4()
        .py_2()
        .min_h(px(40.))
        .rounded_xl()
        .border_1()
        .border_color(rgb(if primary {
            theme::ACCENT
        } else {
            theme::BORDER
        }))
        .bg(rgb(if primary {
            theme::ACCENT
        } else {
            theme::ELEVATED
        }))
        .text_color(rgb(if primary {
            theme::ON_ACCENT
        } else {
            theme::TEXT
        }))
        .text_size(px(14.))
        .line_height(relative(1.4))
        .font_weight(gpui::FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(move |s| s.bg(rgb(if primary { 0xd7d7d7 } else { theme::HOVER })))
        .focus(move |s| {
            s.border_color(rgb(if primary {
                theme::ON_ACCENT
            } else {
                theme::TEXT
            }))
            .bg(rgb(if primary { 0xd7d7d7 } else { theme::HOVER }))
        })
        .on_click(on_click)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .w_full()
                .text_center()
                .child(label.into()),
        )
        .into_any_element()
}

pub fn toggle(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    on_change: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id.into())
        .focusable()
        .tab_stop(true)
        .flex()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap_4()
        .min_h(px(40.))
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(gpui::transparent_black())
        .focus(|s| s.border_color(rgb(theme::TEXT)))
        .cursor_pointer()
        .text_size(px(14.))
        .text_color(rgb(theme::TEXT))
        .child(div().flex_1().min_w_0().child(label.into()))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .w(px(40.))
                .h(px(24.))
                .px(px(3.))
                .rounded_full()
                .border_1()
                .border_color(rgb(theme::CONTROL_BORDER))
                .bg(rgb(if checked {
                    theme::ACCENT
                } else {
                    theme::ELEVATED
                }))
                .when(checked, |s| s.justify_end())
                .child(div().size(px(16.)).rounded_full().bg(rgb(if checked {
                    theme::ON_ACCENT
                } else {
                    theme::SECONDARY
                }))),
        )
        .on_click(move |_, window, cx| on_change(!checked, window, cx))
        .into_any_element()
}

pub fn row(label: impl Into<SharedString>, value: impl Into<SharedString>) -> AnyElement {
    div()
        .flex()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .items_start()
        .justify_between()
        .gap_4()
        .py_1()
        .text_size(px(13.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(rgb(theme::SECONDARY))
                .child(label.into()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_right()
                .text_color(rgb(theme::TEXT))
                .child(value.into()),
        )
        .into_any_element()
}

/// A drag slider stores bounds on the rendered element, not in application globals.
/// Values are clamped in logical pixels, so monitor scale does not change its range.
pub fn slider(
    id: impl Into<ElementId>,
    value: f32,
    min: f32,
    max: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
) -> AnyElement {
    use std::{cell::Cell, rc::Rc};
    let bounds = Rc::new(Cell::new(None::<Bounds<Pixels>>));
    let paint_bounds = bounds.clone();
    let callback = Rc::new(on_change);
    let drag_callback = callback.clone();
    let key_callback = callback.clone();
    let id = id.into();
    let drag_id = id.clone();
    let fraction = if max > min {
        ((value - min) / (max - min)).clamp(0., 1.)
    } else {
        0.
    };
    div()
        .id(id.clone())
        .focusable()
        .tab_stop(true)
        .flex_shrink_0()
        .border_1()
        .border_color(gpui::transparent_black())
        .rounded_md()
        .focus(|s| s.border_color(rgb(theme::TEXT)))
        .on_key_down(move |event, window, cx| {
            let step = (max - min) / 100.;
            let next = match event.keystroke.key.as_str() {
                "left" | "down" => Some(value - step),
                "right" | "up" => Some(value + step),
                "home" => Some(min),
                "end" => Some(max),
                _ => None,
            };
            if let Some(next) = next {
                key_callback(next.clamp(min, max), window, cx);
                cx.stop_propagation();
            }
        })
        .relative()
        .w_full()
        .h(px(32.))
        .cursor_pointer()
        .child(
            gpui::canvas(
                move |b, _, _| {
                    paint_bounds.set(Some(b));
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(13.))
                .h(px(6.))
                .rounded_full()
                .bg(rgb(theme::BORDER)),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .top(px(13.))
                .w(relative(fraction))
                .h(px(6.))
                .rounded_full()
                .bg(rgb(theme::SECONDARY)),
        )
        .child(
            div()
                .absolute()
                .left(relative(fraction))
                .top(px(8.))
                .ml(-px(8.))
                .size(px(16.))
                .border_2()
                .border_color(rgb(theme::SURFACE))
                .rounded_full()
                .bg(rgb(theme::TEXT)),
        )
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            if let Some(b) = bounds.get() {
                callback(slider_value(event.position.x, b, min, max), window, cx);
            }
        })
        .on_drag(SliderDrag(id), |_, _, _, cx| cx.new(|_| SliderGhost))
        .on_drag_move::<SliderDrag>(move |event, window, cx| {
            if event.drag(cx).0 == drag_id {
                drag_callback(
                    slider_value(event.event.position.x, event.bounds, min, max),
                    window,
                    cx,
                );
            }
        })
        .into_any_element()
}

/// A narrow vertical fader keeps all ten EQ bands visible in the game drawer.
pub fn vertical_slider(
    id: impl Into<ElementId>,
    value: f32,
    min: f32,
    max: f32,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
) -> AnyElement {
    use std::{cell::Cell, rc::Rc};
    let id = id.into();
    let drag_id = id.clone();
    let bounds = Rc::new(Cell::new(None::<Bounds<Pixels>>));
    let paint_bounds = bounds.clone();
    let callback = Rc::new(on_change);
    let drag_callback = callback.clone();
    let key_callback = callback.clone();
    let fraction = if max > min {
        ((value - min) / (max - min)).clamp(0., 1.)
    } else {
        0.
    };
    div()
        .id(id.clone())
        .focusable()
        .tab_stop(true)
        .flex_shrink_0()
        .border_1()
        .border_color(gpui::transparent_black())
        .rounded_md()
        .focus(|s| s.border_color(rgb(theme::TEXT)))
        .on_key_down(move |event, window, cx| {
            let step = (max - min) / 100.;
            let next = match event.keystroke.key.as_str() {
                "left" | "down" => Some(value - step),
                "right" | "up" => Some(value + step),
                "home" => Some(min),
                "end" => Some(max),
                _ => None,
            };
            if let Some(next) = next {
                key_callback(next.clamp(min, max), window, cx);
                cx.stop_propagation();
            }
        })
        .relative()
        .w_full()
        .h(px(112.))
        .cursor_pointer()
        .child(
            gpui::canvas(
                move |b, _, _| {
                    paint_bounds.set(Some(b));
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .child(
            div()
                .absolute()
                .left(relative(0.5))
                .ml(-px(2.))
                .top_0()
                .bottom_0()
                .w(px(4.))
                .rounded_full()
                .bg(rgb(0x303030)),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(relative(0.5))
                .h(px(1.))
                .bg(rgb(0x777777)),
        )
        .child(
            div()
                .absolute()
                .left(relative(0.5))
                .ml(-px(7.))
                .bottom(relative(fraction))
                .mb(-px(4.))
                .w(px(14.))
                .h(px(8.))
                .rounded_sm()
                .bg(rgb(0xffffff)),
        )
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            if let Some(b) = bounds.get() {
                callback(vertical_value(event.position.y, b, min, max), window, cx);
            }
        })
        .on_drag(SliderDrag(id), |_, _, _, cx| cx.new(|_| SliderGhost))
        .on_drag_move::<SliderDrag>(move |event, window, cx| {
            if event.drag(cx).0 == drag_id {
                drag_callback(
                    vertical_value(event.event.position.y, event.bounds, min, max),
                    window,
                    cx,
                );
            }
        })
        .into_any_element()
}
fn vertical_value(y: Pixels, bounds: Bounds<Pixels>, min: f32, max: f32) -> f32 {
    if max <= min || bounds.size.height <= px(0.) {
        return min;
    }
    min + ((bounds.bottom() - y) / bounds.size.height).clamp(0., 1.) * (max - min)
}

struct SliderDrag(ElementId);
struct SliderGhost;
impl Render for SliderGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size(px(0.))
    }
}

fn slider_value(x: Pixels, bounds: Bounds<Pixels>, min: f32, max: f32) -> f32 {
    if max <= min || bounds.size.width <= px(0.) {
        return min;
    }
    let fraction = ((x - bounds.left()) / bounds.size.width).clamp(0., 1.);
    min + fraction * (max - min)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_popup_matches_trigger_and_follows_wrapped_height() {
        let viewport = size(px(1200.), px(900.));
        let trigger = Bounds::new(point(px(215.), px(138.)), size(px(736.), px(42.)));
        let popup = select_menu_bounds(trigger, viewport, px(230.));
        assert_eq!(
            popup,
            Bounds::new(point(px(215.), px(184.)), size(px(736.), px(230.)))
        );
        let wrapped = Bounds::new(trigger.origin, size(px(736.), px(57.)));
        assert_eq!(
            select_menu_bounds(wrapped, viewport, px(230.)).top(),
            px(199.)
        );
    }

    #[test]
    fn select_popup_flips_above_and_constrains_narrow_viewports() {
        let trigger = Bounds::new(point(px(215.), px(820.)), size(px(736.), px(42.)));
        let popup = select_menu_bounds(trigger, size(px(1200.), px(900.)), px(230.));
        assert_eq!(
            popup,
            Bounds::new(point(px(215.), px(586.)), size(px(736.), px(230.)))
        );
        let narrow = select_menu_bounds(trigger, size(px(600.), px(900.)), px(230.));
        assert_eq!(narrow.left(), px(12.));
        assert_eq!(narrow.size.width, px(576.));
        let short = select_menu_bounds(
            Bounds::new(point(px(20.), px(220.)), size(px(300.), px(42.))),
            size(px(600.), px(400.)),
            px(280.),
        );
        assert_eq!(short.top(), px(12.));
        assert_eq!(short.size.height, px(204.));
    }

    #[test]
    fn utf16_offsets_never_split_surrogates_or_utf8() {
        let text = "A🎮猫e\u{301}";
        assert_eq!(utf16_to_utf8(text, 0), 0);
        assert_eq!(utf16_to_utf8(text, 1), 1);
        assert_eq!(utf16_to_utf8(text, 2), 1);
        assert_eq!(utf16_to_utf8(text, 3), 5);
        assert_eq!(utf16_to_utf8(text, usize::MAX), text.len());
        for offset in 0..10 {
            assert!(text.is_char_boundary(utf16_to_utf8(text, offset)));
        }
    }

    #[test]
    fn ime_selection_is_relative_to_inserted_text_not_the_document() {
        // Composition follows a three-byte CJK prefix and replaces another CJK word.
        assert_eq!(composition_selection("🎮é", 3, Some(2..3)), 7..9);
        assert_eq!(composition_selection("🎮é", 3, Some(1..99)), 3..9);
        assert_eq!(composition_selection("🎮é", 3, None), 9..9);
    }

    #[test]
    fn bounded_edit_preserves_multibyte_suffix_and_replacement_space() {
        let mut text: SharedString = "A🎮猫".into();
        let inserted = replace_limited(&mut text, 1..5, "é界\nX", 4);
        assert_eq!(inserted, "é界");
        assert_eq!(text.as_ref(), "Aé界猫");
        let end = text.len();
        assert_eq!(replace_limited(&mut text, end..end, "Z", 4), "");
        assert_eq!(text.as_ref(), "Aé界猫");
    }

    #[test]
    fn paste_cannot_inject_lines_or_controls() {
        assert_eq!(
            single_line("one\ntwo\tthree\r\u{0}four\u{2028}five\u{2029}six", 128),
            "one two three four five six"
        );
    }

    #[test]
    fn slider_range_is_scale_independent_and_clamped() {
        let b = Bounds::new(point(px(100.), px(10.)), size(px(200.), px(28.)));
        assert_eq!(slider_value(px(200.), b, -100., 100.), 0.);
        assert_eq!(slider_value(px(-20.), b, -100., 100.), -100.);
        assert_eq!(slider_value(px(500.), b, -100., 100.), 100.);
        assert_eq!(vertical_value(px(10.), b, -12., 12.), 12.);
        assert_eq!(vertical_value(px(24.), b, -12., 12.), 0.);
        assert_eq!(vertical_value(px(100.), b, -12., 12.), -12.);
    }
}
