//! The window, driven without a screen: frames are run, widgets found by their text and
//! clicked, and the engine state is checked. It catches what unit tests cannot: a tool whose
//! tab panics or does not draw, a button that does nothing, ids that clash.

use bluepane_engine::Session;
use bluepane_ui_egui::BluepaneUi;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

/// A window over two folders with a few files, in a private config directory.
fn window() -> (tempfile::TempDir, Harness<'static, BluepaneUi>) {
    let tmp = tempfile::tempdir().unwrap();
    let (left, right) = (tmp.path().join("left"), tmp.path().join("right"));
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    std::fs::write(left.join("a.txt"), "first\nsecond\n").unwrap();
    std::fs::write(left.join("b.txt"), "b").unwrap();
    std::fs::write(right.join("c.txt"), "c").unwrap();
    let mut session = Session::new(&left, &right);
    session.use_config_dir(&tmp.path().join("config"));
    let harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 700.0))
        .build_ui_state(
            |ui, app: &mut BluepaneUi| app.show(ui),
            BluepaneUi::new(session),
        );
    (tmp, harness)
}

/// Runs frames until the free space of the drives has arrived. It comes from a background
/// thread and adds a line under a drive, which moves what is below it: a click aimed at the
/// position of an earlier frame would land on the wrong row.
fn settle(harness: &mut Harness<'_, BluepaneUi>) {
    for _ in 0..300 {
        harness.run_steps(2);
        if !harness.state_mut().session_mut().disk_usage.is_asking() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    harness.run_steps(2);
}

fn command(harness: &mut Harness<'_, BluepaneUi>, line: &str) {
    harness.state_mut().session_mut().run_line(line).unwrap();
    harness.run_steps(4);
}

#[test]
fn the_window_starts_with_two_panels_and_the_function_key_bar() {
    let (_tmp, mut harness) = window();
    harness.run_steps(4);
    // Each panel's footer says what its folder holds (the tab titles are not widgets).
    harness.get_by_label_contains("2 elementy: 0 katalogów, 2 pliki");
    harness.get_by_label_contains("1 element: 0 katalogów, 1 plik");
    // The breadcrumbs name the folders.
    assert!(harness.query_all_by_label("left").count() >= 1);
    assert!(harness.query_all_by_label("right").count() >= 1);
}

#[test]
fn a_tool_opens_as_a_tab_beside_the_panels_and_closes_with_its_command() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "queue");
    harness.get_by_label("Kolejka zadań");
    command(&mut harness, "queueclose");
    assert!(harness.query_by_label("Kolejka zadań").is_none());
    // The panels are still there.
    harness.get_by_label_contains("2 elementy: 0 katalogów, 2 pliki");
}

#[test]
fn a_button_in_a_tool_reaches_the_engine() {
    let (_tmp, mut harness) = window();
    command(&mut harness, "queue");
    assert!(!harness.state().session().paused);
    harness.get_by_label("Wstrzymaj kolejkę").click();
    harness.run_steps(4);
    assert!(
        harness.state().session().paused,
        "the pause button paused the queue"
    );
    harness.get_by_label("Wznów kolejkę").click();
    harness.run_steps(4);
    assert!(!harness.state().session().paused);
}

#[test]
fn several_tools_can_be_open_at_once_and_the_newest_tab_is_shown() {
    let (_tmp, mut harness) = window();
    command(&mut harness, "queue");
    command(&mut harness, "sync");
    command(&mut harness, "find");
    assert_eq!(harness.state().session().open_tools().len(), 3);
    // Tabs of one group show one tool at a time: the one opened last.
    harness.get_by_label("Wyszukiwanie plików");
    assert!(harness.query_by_label("Porównaj (F9)").is_none());
    // Closing it shows another of the open tools, not an empty area.
    command(&mut harness, "findclose");
    assert!(harness.query_by_label("Wyszukiwanie plików").is_none());
    assert_eq!(harness.state().session().open_tools().len(), 2);
    let shown = harness.query_by_label("Porównaj (F9)").is_some()
        || harness.query_by_label("Kolejka zadań").is_some();
    assert!(shown, "one of the remaining tools is drawn");
    // The panels never go away.
    harness.get_by_label_contains("2 elementy: 0 katalogów, 2 pliki");
}

#[test]
fn quick_view_shows_the_file_under_the_cursor_in_the_other_panel() {
    let (_tmp, mut harness) = window();
    // Cursor on a.txt (the row after `..`), then Quick View.
    command(&mut harness, "select a.txt");
    command(&mut harness, "quick");
    // The preview waits for the cursor to rest; give it frames.
    for _ in 0..40 {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(10));
        if harness.query_by_label("a.txt").is_some() {
            break;
        }
    }
    harness.get_by_label("a.txt");
    command(&mut harness, "quick");
}

#[test]
fn every_tool_draws_without_panicking() {
    let (_tmp, mut harness) = window();
    for line in ["sync", "find", "queue", "sites", "scan", "multirename"] {
        // Multi-rename needs something selected.
        if line == "multirename" {
            command(&mut harness, "select a.txt");
        }
        command(&mut harness, line);
        harness.run_steps(3);
    }
    assert_eq!(harness.state().session().open_tools().len(), 6);
}

#[test]
fn the_command_palette_filters_and_runs_a_command() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "palette");
    // The field and the list are on screen; typing narrows the list.
    command(&mut harness, "palette query kolejka");
    harness.get_by_label_contains("queuepause");
    assert!(harness.query_by_label_contains("sortuje listę").is_none());
    command(&mut harness, "palette query queue");
    command(&mut harness, "palette run");
    harness.get_by_label("Kolejka zadań");
    assert!(harness.state_mut().session_mut().palette.is_none());
}

#[test]
fn a_command_with_an_argument_chosen_in_the_palette_fills_the_command_line() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "palette selmask");
    command(&mut harness, "palette run");
    assert!(
        harness.state_mut().session_mut().command_prefill.is_none(),
        "taken by the window"
    );
}

#[test]
fn jump_to_lists_visited_folders_and_goes_to_the_chosen_one() {
    let (tmp, mut harness) = window();
    std::fs::create_dir_all(tmp.path().join("left/dokumenty")).unwrap();
    harness.run_steps(2);
    command(&mut harness, "cd dokumenty");
    command(&mut harness, "cd ..");
    command(&mut harness, "jump");
    harness.get_by_label_contains("dokumenty");
    command(&mut harness, "jump query dokumenty");
    command(&mut harness, "jump run");
    assert!(harness.state_mut().session_mut().jump.is_none());
    assert!(harness
        .state_mut()
        .session_mut()
        .active_panel()
        .cwd
        .ends_with("dokumenty"));
}

#[test]
fn the_places_bar_lists_favorites_and_a_click_goes_there() {
    let (tmp, mut harness) = window();
    std::fs::create_dir_all(tmp.path().join("left/ulubiony")).unwrap();
    harness.run_steps(2);
    command(&mut harness, "fav ulubiony");
    assert!(
        harness.query_by_label("Ulubione").is_none(),
        "the bar is hidden by default"
    );
    command(&mut harness, "places");
    harness.get_by_label("Ulubione");
    harness.get_by_label("Dyski");
    settle(&mut harness);
    harness.get_by_label("ulubiony").click();
    harness.run_steps(4);
    let session = harness.state_mut().session_mut();
    assert!(
        session.active_panel().cwd.ends_with("ulubiony"),
        "cwd {:?}, status: {}",
        session.active_panel().cwd,
        session.status
    );
    command(&mut harness, "places");
    assert!(harness.query_by_label("Ulubione").is_none());
}

#[test]
fn the_path_bar_text_runs_commands_and_offers_completions() {
    let (tmp, mut harness) = window();
    std::fs::create_dir_all(tmp.path().join("left/dokumenty")).unwrap();
    harness.run_steps(2);
    command(&mut harness, "go >queue");
    harness.get_by_label("Kolejka zadań");
    command(&mut harness, "go dokumenty");
    assert!(harness
        .state_mut()
        .session_mut()
        .active_panel()
        .cwd
        .ends_with("dokumenty"));
    command(&mut harness, "go ..");
    let hints = harness.state_mut().session_mut().omnibox_hints("dok");
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].label, "dokumenty");
}

#[test]
fn the_breadcrumb_menu_lists_the_folders_beside_the_current_one() {
    let (tmp, mut harness) = window();
    std::fs::create_dir_all(tmp.path().join("left/a/glebokie")).unwrap();
    std::fs::create_dir_all(tmp.path().join("left/b")).unwrap();
    harness.run_steps(2);
    command(&mut harness, "cd a/glebokie");
    // The window draws the deeper path with its separators without panicking.
    harness.get_by_label_contains("0 elementów");
    let session = harness.state_mut().session_mut();
    let beside: Vec<String> = session
        .crumb_subfolders(0, &tmp.path().join("left"))
        .into_iter()
        .map(|c| c.label)
        .collect();
    assert_eq!(beside, ["a", "b"]);
}

#[test]
fn the_context_menu_offers_actions_for_the_row_and_a_click_runs_one() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "select a.txt");
    command(&mut harness, "unselectall");
    command(&mut harness, "menu");
    // (Podgląd is also on the action bar for a text file, so it is not looked up here.)
    harness.get_by_label_contains("Otwórz w programie");
    harness.get_by_label_contains("Kopiuj do drugiego panelu");
    harness.get_by_label_contains("Właściwości").click();
    harness.run_steps(4);
    assert!(
        harness
            .state_mut()
            .session_mut()
            .status
            .starts_with("a.txt: plik"),
        "{}",
        harness.state_mut().session_mut().status
    );
    assert!(
        harness.query_by_label_contains("Właściwości").is_none(),
        "the menu closed after the choice"
    );
}

#[test]
fn f2_edits_the_name_in_its_row_instead_of_a_dialog() {
    let (tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "select a.txt");
    command(&mut harness, "unselectall");
    command(&mut harness, "rename");
    harness.run_steps(3);
    // The name sits in a text field in the list, and no dialog asks for it.
    assert!(harness.query_all_by_value("a.txt").count() >= 1);
    assert!(harness.query_by_label("OK (Enter)").is_none());
    if let Some(bluepane_engine::Pending::Input { text, .. }) =
        &mut harness.state_mut().session_mut().pending
    {
        *text = "z.txt".into();
    }
    command(&mut harness, "confirm");
    assert!(tmp.path().join("left/z.txt").exists());
    assert!(!tmp.path().join("left/a.txt").exists());
}

#[test]
fn a_finished_job_shows_a_notice_that_can_be_closed() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "select a.txt");
    command(&mut harness, "copy");
    harness.state_mut().session_mut().wait_idle();
    harness.run_steps(4);
    assert_eq!(harness.state().toast_count(), 1);
}

#[test]
fn the_action_bar_offers_what_fits_the_selection_and_its_buttons_work() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    // Two files marked: pack and rename them together.
    command(&mut harness, "selectall");
    harness.get_by_label_contains("Zmień nazwy zbiorowo");
    harness.get_by_label_contains("Spakuj (2)").click();
    harness.run_steps(4);
    assert!(
        matches!(
            harness.state_mut().session_mut().pending,
            Some(bluepane_engine::Pending::Pack(_))
        ),
        "the button ran `pack` (its dialog is open)"
    );
}

#[test]
fn the_key_sheet_lists_the_keys_and_question_mark_does_not_start_a_filter() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "keys");
    harness.get_by_label("Klawisze");
    harness.get_by_label("Ctrl+P");
    command(&mut harness, "keys");
    assert!(harness.query_by_label("Klawisze").is_none());

    harness.key_press(egui::Key::Questionmark);
    harness.event(egui::Event::Text("?".into()));
    harness.run_steps(3);
    harness.get_by_label("Klawisze");
    assert!(
        harness
            .state_mut()
            .session_mut()
            .active_panel()
            .filter
            .is_empty(),
        "? is not typed into the filter"
    );
}

#[test]
fn holding_ctrl_for_a_moment_shows_its_keys_until_it_is_let_go() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::CTRL));
    harness.run_steps(80);
    harness.get_by_label("Klawisze z Ctrl");
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    harness.run_steps(3);
    assert!(harness.query_by_label("Klawisze z Ctrl").is_none());
}

#[test]
fn a_picture_opens_in_the_page_view_and_is_drawn() {
    let (tmp, mut harness) = window();
    // A minimal valid 1 × 1 PNG, written byte by byte (the UI crate does not decode images).
    let png: [u8; 67] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    std::fs::write(tmp.path().join("left/punkt.png"), png).unwrap();
    harness.run_steps(2);
    command(&mut harness, "refresh");
    command(&mut harness, "select punkt.png");
    command(&mut harness, "unselectall");
    command(&mut harness, "view");
    harness.run_steps(20);
    let session = harness.state_mut().session_mut();
    assert!(
        session.pages.as_ref().is_some_and(|p| p.pages() == 1),
        "{}",
        session.status
    );
}

#[test]
fn the_grid_view_draws_tiles_and_the_arrows_move_by_rows() {
    let (tmp, mut harness) = window();
    for i in 0..12 {
        std::fs::write(tmp.path().join(format!("left/plik{i:02}.txt")), "x").unwrap();
    }
    harness.run_steps(2);
    command(&mut harness, "refresh");
    command(&mut harness, "viewmode grid");
    harness.run_steps(4);
    assert_eq!(
        harness.state_mut().session_mut().active_panel().view,
        bluepane_engine::PanelView::Grid
    );
    let before = harness.state_mut().session_mut().active_panel().cursor;
    harness.key_press(egui::Key::ArrowRight);
    harness.run_steps(2);
    assert_eq!(
        harness.state_mut().session_mut().active_panel().cursor,
        before + 1
    );
    harness.key_press(egui::Key::ArrowDown);
    harness.run_steps(2);
    let after = harness.state_mut().session_mut().active_panel().cursor;
    assert!(
        after > before + 1,
        "down moves by a whole row of tiles: {before} -> {after}"
    );
    command(&mut harness, "viewmode");
    assert_eq!(
        harness.state_mut().session_mut().active_panel().view,
        bluepane_engine::PanelView::List
    );
}

#[test]
fn chosen_columns_and_compact_rows_draw_and_are_kept() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "columns created");
    command(&mut harness, "columns ext");
    command(&mut harness, "density compact");
    harness.run_steps(4);
    let session = harness.state_mut().session_mut();
    assert_eq!(
        session.columns(),
        [
            bluepane_engine::Column::Size,
            bluepane_engine::Column::Created,
            bluepane_engine::Column::Modified
        ]
    );
    assert_eq!(session.density(), bluepane_engine::Density::Compact);
    // The panels still list their files.
    harness.get_by_label_contains("2 elementy: 0 katalogów, 2 pliki");
}

#[test]
fn a_layout_is_saved_by_name_reset_and_loaded_again() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "queue");
    command(&mut harness, "layout save Praca");
    assert_eq!(harness.state_mut().session_mut().saved_layouts(), ["Praca"]);
    command(&mut harness, "layout reset");
    command(&mut harness, "layout load Praca");
    harness.run_steps(2);
    let session = harness.state_mut().session_mut();
    assert!(session.status.contains("Praca"), "{}", session.status);
    assert!(!session.status_is_error);
    // The panels are still there after all that.
    harness.get_by_label_contains("2 elementy: 0 katalogów, 2 pliki");
}

#[test]
fn the_one_box_shows_commands_and_files_and_enter_goes_to_the_file() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "box a.txt");
    harness.get_by_label_contains("Plik  a.txt");
    command(&mut harness, "box move 1000");
    command(&mut harness, "box run");
    harness.run_steps(2);
    let session = harness.state_mut().session_mut();
    assert!(session.quickbox.is_none());
    assert_eq!(session.active_panel().current().unwrap().name, "a.txt");
}

#[test]
fn appearance_settings_change_the_window_without_breaking_it() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "appearance textsize 140");
    command(&mut harness, "appearance rowheight 30");
    command(&mut harness, "appearance accent #ff8800");
    harness.run_steps(3);
    let session = harness.state_mut().session_mut();
    assert_eq!(session.text_size(), 140);
    assert_eq!(session.accent(), Some([255, 136, 0]));
    harness.get_by_label_contains("2 elementy: 0 katalogów, 2 pliki");
    command(&mut harness, "appearance reset");
    harness.get_by_label_contains("2 elementy: 0 katalogów, 2 pliki");
}

#[test]
fn the_light_and_system_themes_can_be_chosen_and_the_window_keeps_working() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    for name in ["light", "system", "dark"] {
        command(&mut harness, &format!("theme {name}"));
        harness.run_steps(2);
        assert_eq!(harness.state_mut().session_mut().theme(), name);
        harness.get_by_label_contains("2 elementy: 0 katalogów, 2 pliki");
    }
    assert!(harness
        .state_mut()
        .session_mut()
        .run_line("theme neon")
        .is_err());
}

#[test]
fn permission_and_owner_columns_and_dragged_widths_leave_the_list_working() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "columns perms");
    command(&mut harness, "columns owner");
    command(&mut harness, "columnwidth size 150");
    harness.run_steps(3);
    harness.get_by_label_contains("2 elementy: 0 katalogów, 2 pliki");
    let session = harness.state_mut().session_mut();
    assert_eq!(
        session.column_width(bluepane_engine::Column::Size),
        Some(150)
    );
    assert!(session.columns().contains(&bluepane_engine::Column::Perms));
    command(&mut harness, "columnwidth reset");
    command(&mut harness, "columns default");
    harness.get_by_label_contains("2 elementy: 0 katalogów, 2 pliki");
}

#[test]
fn a_workspace_is_saved_with_its_layout_and_loaded_again() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "queue");
    command(&mut harness, "workspace save Praca");
    harness.run_steps(2);
    assert_eq!(
        harness.state_mut().session_mut().saved_workspaces(),
        ["Praca"]
    );
    assert_eq!(
        harness.state_mut().session_mut().saved_layouts(),
        ["obszar Praca"]
    );
    command(&mut harness, "layout reset");
    command(&mut harness, "workspace load Praca");
    harness.run_steps(2);
    let session = harness.state_mut().session_mut();
    assert!(!session.status_is_error, "{}", session.status);
    harness.get_by_label_contains("2 elementy: 0 katalogów, 2 pliki");
}

#[test]
fn quick_view_in_a_tab_leaves_both_panels_in_place() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "quick tab");
    command(&mut harness, "select a.txt");
    command(&mut harness, "unselectall");
    command(&mut harness, "quick");
    harness.run_steps(6);
    // The right panel is still shown (its footer), not replaced by the preview.
    harness.get_by_label_contains("1 element: 0 katalogów, 1 plik");
    assert!(harness.state_mut().session_mut().quick.is_some());
    command(&mut harness, "quick");
    assert!(harness.state_mut().session_mut().quick.is_none());
}

#[test]
fn a_filter_with_criteria_narrows_the_list_and_the_footer_counts_it() {
    let (_tmp, mut harness) = window();
    harness.run_steps(2);
    command(&mut harness, "filter pliki ext:txt a");
    harness.get_by_label_contains("Widoczne 1 z 2");
    command(&mut harness, "filter");
    harness.get_by_label_contains("2 elementy: 0 katalogów, 2 pliki");
}
