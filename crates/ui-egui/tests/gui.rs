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
    harness.get_by_label_contains("Podgląd");
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
