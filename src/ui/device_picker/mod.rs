//! Device picker modal — the overlay that lists discovered devices and
//! connected apps, lets the user select an app to focus, and surfaces
//! connection state.
//!
//! Split across submodules (Phase 3 UI-038 mirror):
//! * [`modal`]    — outer block, empty state, scrollbar frame helpers
//! * [`row`]      — device container rows (top/bottom/blank/waiting)
//! * [`card`]     — app card render (top, details, bottom)
//! * [`click_map`] — scroll-clamp + click-region extraction
//! * `mod.rs`     — public entry point [`draw_device_picker`] that wires
//!   the submodules together.

mod card;
mod click_map;
mod modal;
mod palette;
mod row;

use ratatui::{layout::Rect, Frame};

use crate::app::App;
use crate::transport::device_monitor::DeviceKind;

use card::push_app_card;
use palette::BASE;
use row::{
    push_device_bottom, push_device_inner_blank, push_device_top, push_waiting_row, shorten_id,
    PickerLine,
};

/// Draw the device picker as a centered modal. Each device is a rounded container
/// that visually wraps its app cards. Active app uses double-line border.
pub fn draw_device_picker(f: &mut Frame, app: &mut App, area: Rect) {
    // ── Build tree: group apps by device_id ──
    let mut device_order: Vec<String> = Vec::new();
    let mut apps_by_device: std::collections::HashMap<String, Vec<crate::app::ConnectedApp>> =
        std::collections::HashMap::new();
    for ca in &app.connected_apps {
        apps_by_device
            .entry(ca.device_id.clone())
            .or_default()
            .push(ca.clone());
        if !device_order.contains(&ca.device_id) {
            device_order.push(ca.device_id.clone());
        }
    }
    for dev in app.discovered_devices.values() {
        if !device_order.contains(&dev.id) {
            device_order.push(dev.id.clone());
        }
    }
    let device_count = device_order.len();

    // ── Size + outer block ──
    let picker_area = modal::compute_modal_area(area);
    modal::clear_modal_area(f, picker_area);
    let block = modal::build_modal_block(device_count);
    let inner = block.inner(picker_area);
    let inner_w = inner.width as usize;
    let inner_h = inner.height as usize;

    // ── Empty state ──
    if device_order.is_empty() {
        modal::render_empty_state(f, picker_area, block, inner_w, inner_h);
        app.layout.device_picker_items = Vec::new();
        app.layout.device_picker_rect = Some((
            picker_area.x,
            picker_area.y,
            picker_area.width,
            picker_area.height,
        ));
        app.layout.device_picker_item_ids = Vec::new();
        app.layout.device_picker_total_lines = 0;
        return;
    }

    // ── Build content lines ──
    //
    // Layout (widths inside `inner`):
    //   col 0            → modal bg (BASE gutter)
    //   col 1 .. dev_w+1 → device container (width = inner_w - 2)
    //   col dev_w+1 ..   → modal bg (BASE gutter)
    //
    // Inside a device container (width dev_w):
    //   col 0            → device │
    //   col 1 .. 4       → card gutter (3 spaces of MANTLE) — actually 3 cols
    //   col 4 .. dev_w-4 → app card (width = dev_w - 7)
    //   col dev_w-4 .. dev_w-1 → card right gutter (3 spaces of MANTLE)
    //   col dev_w-1      → device │
    //
    // Card widths must be ≥ 20 or we fall back to no-indent.

    let device_gutter = 1u16; // cols of BASE on each side of device container
    let dev_w = inner_w.saturating_sub(2 * device_gutter as usize);

    let card_indent = 3u16; // MANTLE gutter inside device container, left & right of card
    let card_w = dev_w.saturating_sub(2 * card_indent as usize);

    let mut lines: Vec<PickerLine> = Vec::new();
    let mut selectable_ids: Vec<String> = Vec::new();

    // One blank line of BASE at top for breathing room.
    lines.push(PickerLine::plain(inner_w, BASE));

    for (di, device_id) in device_order.iter().enumerate() {
        let dev = app.discovered_devices.get(device_id);
        let dev_name = dev
            .map(|d| d.name.clone())
            .unwrap_or_else(|| device_id.clone());
        let dev_kind = dev.map(|d| d.kind.clone()).unwrap_or(DeviceKind::Local);

        // Device header: platform tag + name + connection + short id
        let (platform_tag, conn_label) = match &dev_kind {
            DeviceKind::Android => ("Android", "ADB"),
            DeviceKind::IosUsb { .. } => ("iOS", "USB"),
            DeviceKind::Local => ("Simulator", "localhost"),
        };

        // Short id for display (device_id is typically the UDID / hostname key).
        let id_short = shorten_id(device_id);

        // Device top border: `╭─ [iOS] iPhone 17 ─── USB · 00008150...401C ─╮`
        push_device_top(
            &mut lines,
            inner_w,
            dev_w,
            device_gutter,
            platform_tag,
            &dev_name,
            conn_label,
            &id_short,
        );

        // Inside-device blank row
        push_device_inner_blank(&mut lines, inner_w, dev_w, device_gutter);

        // App cards (or Waiting)
        let list = apps_by_device.get(device_id);
        match list {
            Some(apps) if !apps.is_empty() => {
                for (ai, ca) in apps.iter().enumerate() {
                    let sel_idx = selectable_ids.len();
                    selectable_ids.push(ca.id.clone());
                    let is_active = app.active_app_id.as_deref() == Some(&ca.id);
                    let is_selected = app.device_picker_selected == sel_idx;

                    push_app_card(
                        &mut lines,
                        inner_w,
                        dev_w,
                        device_gutter,
                        card_indent,
                        card_w,
                        sel_idx,
                        is_active,
                        is_selected,
                        &ca.app_name,
                        &ca.app_version,
                        &ca.package_name,
                        &ca.os,
                        &ca.build_mode,
                        ca.port,
                    );

                    // Blank inside device between cards (but not after last)
                    if ai + 1 < apps.len() {
                        push_device_inner_blank(&mut lines, inner_w, dev_w, device_gutter);
                    }
                }
            }
            _ => {
                // Waiting row inside device container
                push_waiting_row(&mut lines, inner_w, dev_w, device_gutter);
            }
        }

        // Inside-device blank row before bottom border
        push_device_inner_blank(&mut lines, inner_w, dev_w, device_gutter);

        // Device bottom border
        push_device_bottom(&mut lines, inner_w, dev_w, device_gutter);

        // Blank line of BASE between devices
        if di + 1 < device_order.len() {
            lines.push(PickerLine::plain(inner_w, BASE));
        }
    }

    // Trailing BASE blank for breathing room
    lines.push(PickerLine::plain(inner_w, BASE));

    // ── Clamp selection + scroll ──
    let scroll = click_map::clamp_scroll(
        &lines,
        &mut app.device_picker_selected,
        &mut app.device_picker_scroll,
        selectable_ids.len(),
        inner_h,
    );

    // ── Render block + content ──
    f.render_widget(block, picker_area);

    let viewport = click_map::build_viewport(
        &lines,
        inner,
        inner_w,
        dev_w,
        device_gutter,
        card_indent,
        &scroll,
    );

    f.render_widget(ratatui::widgets::Paragraph::new(viewport.out_lines), inner);

    // Scrollbar
    if scroll.total_lines > scroll.visible_h {
        modal::render_scrollbar(f, inner, scroll.max_scroll, scroll.scroll_offset);
    }

    app.layout.device_picker_items = viewport.click_regions;
    app.layout.device_picker_rect = Some((
        picker_area.x,
        picker_area.y,
        picker_area.width,
        picker_area.height,
    ));
    app.layout.device_picker_item_ids = selectable_ids;
    app.layout.device_picker_total_lines = scroll.total_lines;
}
