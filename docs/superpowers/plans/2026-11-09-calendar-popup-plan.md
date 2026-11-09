# Calendar Popup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 3 dropdown date selector (year/month/day) in GalleryTab with a Calendar Popup component — a grid-based month view that opens on click, supports keyboard navigation, and returns selected date.

**Architecture:** New `CalendarPopup` component in `theme.slint` using `PopupWindow` + `Flickable` grid. Replaces 3 `FlatSelect` dropdowns in `gallery_tab.slint`. Single `selected-date` property (ISO string "YYYY-MM-DD" or empty) replaces `filter-year`, `filter-month`, `filter-day`, `filter-year-index`, `filter-month-index`, `filter-day-index`.

**Tech Stack:** Slint 1.18, `PopupWindow`, `Flickable`, `HorizontalLayout`/`VerticalLayout` grid, `TouchArea` for clicks, `FocusScope` for keyboard nav.

**Spec:** Internal design (this plan replaces 3 dropdowns in gallery_tab.slint with single CalendarPopup)

## Global Constraints

- Slint 1.18 (no `DatePicker` in std-widgets)
- Flat theme: zero gradients, zero animations, solid colors only (`#141414`, `#EB4141`, `#22C55E`, etc.)
- Vector icons only (Path), no emojis
- Keyboard accessible (Tab, arrows, Enter, Esc)
- Click-through outside closes popup
- ISO 8601 date format "YYYY-MM-DD" for selected-date
- Empty string = "Todos os dias" (no filter)

## Review Focus

| Input/Failure Mode | Expected Behavior | Test Location |
|--------------------|-------------------|---------------|
| Click outside popup | Popup closes, selection unchanged | `overlay.rs::test_calendar_click_outside` |
| Esc key | Popup closes, selection unchanged | `overlay.rs::test_calendar_esc_key` |
| Arrow keys navigation | Moves selection visually, updates internal state | `overlay.rs::test_calendar_arrow_navigation` |
| Enter on day | Selects day, closes popup, emits `date-selected("YYYY-MM-DD")` | `overlay.rs::test_calendar_enter_selects` |
| Month navigation (◀/▶) | Updates grid to prev/next month | `overlay.rs::test_calendar_month_nav` |
| Invalid date (Feb 30) | Not rendered / skipped in grid | `overlay.rs::test_calendar_invalid_dates` |
| "Hoje" button | Selects today, closes popup | `overlay.rs::test_calendar_today_button` |
| "Limpar" button | Emits `date-selected("")`, closes popup | `overlay.rs::test_calendar_clear_button` |

---

### Task 1: Create CalendarPopup Component in theme.slint

**Files:**
- Modify: `crates/luna-stream-ui/ui/theme.slint` (add after `FlatSelect`)

**Interfaces:**
- Consumes: `FlatSelect` patterns, `PopupWindow`, `Flickable`, `Theme` colors
- Produces: `export component CalendarPopup` with:
  - `in-out property <string> selected-date: ""` (ISO "YYYY-MM-DD" or "")
  - `callback date-selected(string)` (emits ISO or "")
  - `in property <bool> show-today-button: true`
  - `in property <bool> show-clear-button: true`

- [ ] **Step 1: Write failing test for CalendarPopup rendering**

```rust
// tests/calendar_popup.rs
#[test]
fn test_calendar_popup_renders_grid() {
    i_slint_backend_testing::init_no_event_loop();
    let calendar = CalendarPopup::new().unwrap();
    calendar.set_selected_date("2026-11-08".into());
    calendar.show().unwrap();
    // Verify grid has 7 columns × 6 rows = 42 cells
    // Verify header shows "Novembro 2026"
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p luna-stream-ui --test calendar_popup`
Expected: FAIL - component not found

- [ ] **Step 3: Implement `CalendarPopup` component in `theme.slint`**

```slint
// After FlatSelect component (~line 330)
component CalendarDay inherits Rectangle {
    in property <string> date: ""; // "YYYY-MM-DD"
    in property <bool> current-month: true;
    in property <bool> is-today: false;
    in property <bool> selected: false;
    callback clicked();
    // ... styling: 36x36, rounded, hover/selected states
}

export component CalendarPopup inherits Rectangle {
    in-out property <string> selected-date: "";
    in property <bool> show-today-button: true;
    in property <bool> show-clear-button: true;
    callback date-selected(string);

    // Internal state
    property <int> view-year: 2026;
    property <int> view-month: 11; // 1-12
    property <string> selected-day: "";

    // Header with month/year + nav buttons
    // Grid: 7 cols (Do-Sá) × 6 rows
    // Footer: [Hoje] [Limpar]

    popup := PopupWindow { ... }
}
```

Key implementation details:
- `view-year`/`view-month` track displayed month (not selected date)
- Grid generates 42 days: offset for 1st weekday + days in month + next month padding
- `date-selected("")` emitted on Clear, `date-selected("YYYY-MM-DD")` on day click
- Keyboard: arrows move selection, Enter confirms, Esc closes, ◀/▶ month nav

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p luna-stream-ui --test calendar_popup`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/luna-stream-ui/ui/theme.slint tests/calendar_popup.rs
git commit -m "feat(ui): add CalendarPopup component with month grid, keyboard nav, today/clear buttons"
```

---

### Task 2: Replace 3 Dropdowns with CalendarPopup in GalleryTab

**Files:**
- Modify: `crates/luna-stream-ui/ui/gallery_tab.slint` (lines ~135-185)
- Modify: `crates/luna-stream-ui/ui/app.slint` (remove 6 filter props, add 1)

**Interfaces:**
- Consumes: `CalendarPopup` from `theme.slint`
- Produces: Single `filter-date` property replacing 6 props

**Changes in gallery_tab.slint:**
- Remove: `filter-year`, `filter-month`, `filter-day`, `filter-year-index`, `filter-month-index`, `filter-day-index`, `month-options`, `day-options`
- Add: `in-out property <string> filter-date: ""` (ISO or "")
- Replace 3 `FlatSelect` with 1 `CalendarPopup`:
  ```slint
  CalendarPopup {
      width: 120px;
      selected-date: root.filter-date;
      date-selected(d) => {
          root.filter-date = d;
          root.filter-changed(root.filter-kind, root.filter-format, d, "", "");
      }
  }
  ```
- Update `filter-changed` callback to accept single date string

**Changes in app.slint:**
- Remove 6 props, add `filter-date` binding to GalleryTab

- [ ] **Step 1: Write failing test for date filter integration**

```rust
// tests/ui_headless.rs (add to existing)
#[test]
fn test_calendar_popup_filters_gallery() {
    // Set filter-date to "2026-11-08"
    // Verify gallery shows only items from that date
    // Clear filter -> shows all
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p luna-stream-ui --test ui_headless`
Expected: FAIL - props missing

- [ ] **Step 3: Update gallery_tab.slint and app.slint**

Replace lines 135-185 in gallery_tab.slint with CalendarPopup.
Update app.slint: remove 6 filter props, add `filter-date <=> root.filter-date;`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p luna-stream-ui --test ui_headless`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/luna-stream-ui/ui/gallery_tab.slint crates/luna-stream-ui/ui/app.slint
git commit -m "feat(ui): replace 3 dropdowns with CalendarPopup in GalleryTab"
```

---

### Task 3: Update Filter Logic in main.rs

**Files:**
- Modify: `crates/luna-stream-ui/src/main.rs` (filter callback)

**Interfaces:**
- Consumes: `filter-date` string from UI
- Produces: `GalleryFilter` with parsed year/month/day

**Changes:**
```rust
app.on_filter_changed(move |kind, format, date| {
    let (year, month, day) = if date.is_empty() {
        (None, None, None)
    } else {
        let parts: Vec<&str> = date.split('-').collect();
        (parts[0].parse().ok(), parts[1].parse().ok(), parts[2].parse().ok())
    };
    // Send GalleryFilter to supervisor
});
```

- [ ] **Step 1: Write failing test**

```rust
// tests/supervisor_ipc.rs (add)
#[test]
fn test_filter_date_parsing() {
    // Send "2026-11-08" -> verify GalleryFilter { year: 2026, month: 11, day: 8 }
    // Send "" -> verify GalleryFilter { year: None, month: None, day: None }
}
```

- [ ] **Step 2: Update main.rs callback**

- [ ] **Step 3: Run tests**

Run: `cargo test -p luna-stream-ui --test supervisor_ipc`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/luna-stream-ui/src/main.rs
git commit -m "feat: parse filter-date ISO string in main.rs filter callback"
```

---

### Task 4: Update Supervisor and Snapshot

**Files:**
- Modify: `crates/luna-stream-ui/src/lib.rs` (apply_snapshot_to_window)
- Modify: `crates/luna-stream-ui/src/supervisor.rs` (UiSnapshot filter fields)

**Interfaces:**
- Consumes: `filter-date` from UI
- Produces: `UiSnapshot` with parsed filter

**Changes:**
- `supervisor.rs`: Replace `filter: GalleryFilter` in `UiSnapshot` with `filter-date: String`
- `lib.rs`: In `apply_snapshot_to_window`, compute ISO string from `snap.filter` and call `window.set_filter_date(...)`

- [ ] **Step 1: Update supervisor.rs UiSnapshot**

- [ ] **Step 2: Update lib.rs apply_snapshot_to_window**

- [ ] **Step 3: Run all tests**

Run: `cargo test -p luna-stream-ui`
Expected: All 28 tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/luna-stream-ui/src/lib.rs crates/luna-stream-ui/src/supervisor.rs
git commit -m "feat: sync filter-date through supervisor snapshot"
```

---

### Task 5: Add Comprehensive Tests

**Files:**
- Create: `tests/calendar_popup.rs` (unit tests)
- Modify: `tests/ui_headless.rs` (integration)
- Modify: `tests/supervisor_ipc.rs` (IPC)

**Test Cases:**
1. `test_calendar_grid_renders` - 42 cells, correct month header
2. `test_calendar_month_nav` - ◀/▶ updates view-month/view-year
3. `test_calendar_day_click_emits_date` - click day -> `date-selected("2026-11-08")`
4. `test_calendar_today_button` - selects today
5. `test_calendar_clear_button` - emits `""`
6. `test_calendar_keyboard_nav` - arrows, Enter, Esc
6. `test_calendar_invalid_dates_skipped` - Feb 30 not rendered
7. `test_calendar_click_outside_closes` - PopupWindow close-policy
8. `test_calendar_integration_filter_gallery` - filter-date filters items

- [ ] **Step 1: Write all test cases**

- [ ] **Step 2: Run tests**

Run: `cargo test -p luna-stream-ui`
Expected: All PASS

- [ ] **Step 3: Commit**

```bash
git add tests/calendar_popup.rs tests/ui_headless.rs tests/supervisor_ipc.rs
git commit -m "test: add comprehensive CalendarPopup tests"
```

---

## Summary

| Task | Component | Files |
|------|-----------|-------|
| 1 | CalendarPopup | theme.slint (+test) |
| 2 | GalleryTab integration | gallery_tab.slint, app.slint |
| 3 | Filter callback | main.rs |
| 4 | Supervisor sync | lib.rs, supervisor.rs |
| 5 | Tests | calendar_popup.rs, ui_headless.rs, supervisor_ipc.rs |

**Total:** ~5 tasks, each independently testable. CalendarPopup is reusable for future date inputs.