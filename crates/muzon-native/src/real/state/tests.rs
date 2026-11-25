use super::*;
fn app_fixture() -> RealApp {
    let mut snapshot = Snapshot::empty();
    let mut tracks = vec![track(42), track(99), track(7)];
    tracks[1].is_favorite = true;
    tracks[2].is_favorite = true;
    snapshot.library = Arc::new(tracks);
    snapshot.playlists = Arc::new(vec![muzon_core::domain::playlist::Playlist {
        id: 10,
        name: "Mix".into(),
        track_ids: vec![7, 42, 123],
    }]);
    let mut app = RealApp {
        controller: None,
        desktop: None,
        tray: None,
        hidden: false,
        _profile_lock: File::open("/dev/null").unwrap(),
        window: crate::real::window_state::WindowMemory::new(Path::new(
            "/does-not-exist/muzon-test",
        )),
        audio: AudioUi::new(&snapshot),
        services: crate::real::services::Services::new(None, None),
        services_hub: None,
        hotkeys: crate::real::hotkeys::Hotkeys::new(Path::new("/does-not-exist/muzon-test")),
        appearance: crate::real::appearance::Appearance::new(Path::new(
            "/does-not-exist/muzon-test",
        )),
        spectrum: Arc::new(Vec::new()),
        spectrum_enabled: false,
        editor: None,
        palette: None,
        queue_drag: None,
        editor_generation: 0,
        held_keys: HashSet::new(),
        snapshot: Arc::new(snapshot),
        media: Arc::new(Media::default()),
        cover: None,
        screen: Screen::Library,
        query: String::new(),
        smart: crate::real::smart::Smart::MostPlayed,
        smart_refresh: std::time::Instant::now(),
        album_grid: crate::real::album_grid::Grid::new(std::path::PathBuf::new()),
        groups: Vec::new(),
        group_rows: Vec::new(),
        selected_group: None,
        group_scroll: 0.0,
        filtered: Vec::new(),
        search_index: Vec::new(),
        library_by_id: HashMap::new(),
        selected_playlist: None,
        new_playlist_name: String::new(),
        rename_playlist_name: String::new(),
        collection_overlay: None,
        playlist_busy: false,
        pending_favorites: HashSet::new(),
        playlist_scroll: 0.0,
        picker_scroll: 0.0,
        folder_input: String::new(),
        folder_picker_open: false,
        track_covers: crate::real::track_covers::Covers::new(std::path::PathBuf::new()),
        library_scroll: 0.0,
        queue_scroll: 0.0,
        seek_preview: None,
        closing: false,
        audio_close_failed: false,
        ui_error: None,
    };
    app.rebuild_search();
    app
}
#[test]
fn a_failed_sound_save_cancels_close_and_keeps_the_worker_available() {
    use muzon_core::{
        domain::playback_settings::PlaybackEdit,
        testing::{gst_lock, scratch_dir},
    };
    let _gst = gst_lock();
    let dir = scratch_dir("native-ui-close-audio-save");
    std::fs::create_dir_all(dir.join("config/playback_settings.json")).unwrap();
    let core = Controller::spawn_test(CoreConfig {
        db: Arc::new(Db::open(&dir.join("library.sqlite3")).unwrap()),
        session_store: SessionStore::new(&dir.join("config")),
        playback_settings_store: PlaybackSettingsStore::new(&dir.join("config")),
        cover_dir: dir.join("covers"),
    })
    .unwrap();
    core.preview_playback(PlaybackEdit::Crossfade(3.0));
    let mut app = app_fixture();
    app.controller = Some(core);
    let _ = app.update(Message::WindowCloseRequested(None));
    assert!(app.closing);
    let result = app.controller.as_ref().unwrap().save_audio();
    assert!(result.is_err());
    let _ = app.update(Message::ShutdownAudioSaved(None, result));
    assert!(!app.closing);
    assert!(app.audio_close_failed);
    assert!(app.controller.as_ref().unwrap().is_running());
    assert!(app.snapshot.audio_dirty);
    assert!(app.ui_error.is_some());
    std::fs::remove_dir(dir.join("config/playback_settings.json")).unwrap();
    let _ = app.update(Message::AudioSave);
    let result = app.controller.as_ref().unwrap().save_audio();
    assert!(result.is_ok());
    let _ = app.update(Message::AudioSaved(result));
    assert!(app.ui_error.is_none());
    assert!(!app.snapshot.audio_dirty);
    assert!(!app.audio_close_failed);
}

#[test]
fn favorite_and_playlist_views_preserve_identity_order_and_the_playing_queue() {
    let mut app = app_fixture();
    let queue = app.snapshot.queue.clone();
    let _ = app.update(Message::ScreenSelected(Screen::Favorites));
    assert_eq!(app.filtered, vec![1, 2]);
    let _ = app.update(Message::SearchChanged("99".into()));
    assert_eq!(app.filtered, vec![1]);
    let _ = app.update(Message::ScreenSelected(Screen::Playlists));
    assert!(
        app.filtered.is_empty(),
        "search remains active in the selected playlist"
    );
    let _ = app.update(Message::SearchChanged(String::new()));
    assert_eq!(
        app.filtered,
        vec![2, 0],
        "membership is mapped by id, missing tracks are omitted"
    );
    assert_eq!(filtered_track_id(&app.snapshot, &app.filtered, 0), Some(7));
    assert!(Arc::ptr_eq(&queue, &app.snapshot.queue));
    let _ = app.update(Message::PlaylistSelected(999));
    assert!(app.filtered.is_empty());
}
#[test]
fn collection_screens_and_modal_lists_build_without_scroll_axis_fill() {
    let mut app = app_fixture();
    for screen in [
        Screen::Library,
        Screen::Favorites,
        Screen::Playlists,
        Screen::Audio,
        Screen::Sleep,
        Screen::Shortcuts,
        Screen::Appearance,
        Screen::Cloud,
        Screen::Scrobble,
        Screen::Smart,
        Screen::Albums,
        Screen::Artists,
        Screen::Genres,
    ] {
        let _ = app.update(Message::ScreenSelected(screen));
        let _ = crate::real::view::view(&app);
    }
    for overlay in [
        CollectionOverlay::AddTrack(42),
        CollectionOverlay::DeletePlaylist(10),
    ] {
        app.collection_overlay = Some(overlay);
        let _ = crate::real::view::view(&app);
    }
    let _ = app.update(Message::CloseCollectionOverlay);
    assert!(app.collection_overlay.is_none());
    let _ = app.update(Message::AskDeletePlaylist(10));
    app.playlist_busy = true;
    let _ = app.update(Message::CloseCollectionOverlay);
    assert!(
        app.collection_overlay.is_some(),
        "an in-flight deletion cannot be presented as cancelled"
    );
}

fn track(id: i32) -> muzon_core::domain::Track {
    muzon_core::domain::Track {
        id,
        path: format!("/{id}.wav"),
        title: format!("Track {id}"),
        artist: None,
        album: None,
        duration_secs: Some(10.0),
        track_no: None,
        genre: None,
        year: None,
        is_favorite: false,
        tempo: 1.0,
        play_count: 0,
        last_played_at: None,
        added_at: 0.0,
    }
}
#[test]
fn batch_palette_smart_lists_shortcuts_and_online_results_are_isolated() {
    let mut app = app_fixture();
    {
        let snapshot = Arc::make_mut(&mut app.snapshot);
        let library = Arc::make_mut(&mut snapshot.library);
        library[0].play_count = 4;
        library[0].last_played_at = Some(1.);
        library[1].play_count = 0;
        library[2].play_count = 2;
    }
    app.rebuild_search();
    let queue = app.snapshot.queue.clone();
    let _ = app.update(Message::ScreenSelected(Screen::Smart));
    assert_eq!(app.filtered, vec![0, 2]);
    let _ = app.update(Message::SmartSelected(
        crate::real::smart::Smart::NeverPlayed,
    ));
    assert_eq!(app.filtered, vec![1]);
    assert!(Arc::ptr_eq(&queue, &app.snapshot.queue));
    let _ = app.update(Message::Shortcut(crate::real::shortcuts::Shortcut::Palette));
    assert!(app.palette.is_some());
    let _ = app.update(Message::PaletteQuery("таймер сна".into()));
    let _ = app.update(Message::PaletteExecute);
    assert_eq!(app.screen, Screen::Sleep);
    let mut editor = crate::real::editor::Editor::new(
        5,
        muzon_core::editing::EditorSource {
            track: track(42),
            lyrics: "my draft".into(),
            lyrics_in_tag: false,
        },
    );
    editor.loading = false;
    editor.lyrics_dirty = true;
    editor.fetch_serial = 2;
    let (_, handle) = Task::<Message>::future(std::future::pending()).abortable();
    let observer = handle.clone();
    editor.fetch = Some(handle.abort_on_drop());
    app.editor = Some(editor);
    let _ = app.update(Message::EditorFetched(
        5,
        1,
        Ok(Some(muzon_core::data::lrclib::Suggestion {
            text: "stale".into(),
            synced: false,
        })),
    ));
    assert!(app.editor.as_ref().unwrap().suggestion.is_none());
    let _ = app.update(Message::EditorFetched(
        5,
        2,
        Ok(Some(muzon_core::data::lrclib::Suggestion {
            text: "found".into(),
            synced: false,
        })),
    ));
    assert_eq!(
        app.editor.as_ref().unwrap().lyrics.text().trim(),
        "my draft"
    );
    assert!(observer.is_aborted());
    let _ = crate::real::view::view(&app);
    let _ = app.update(Message::EditorUseSuggestion);
    assert_eq!(app.editor.as_ref().unwrap().lyrics.text().trim(), "found");
    let _ = app.update(Message::EditorTapStart);
    assert!(app.editor.as_ref().unwrap().tap.is_some());
    let _ = crate::real::view::view(&app);
    let _ = app.update(Message::EditorTapCancel);
    assert_eq!(app.editor.as_ref().unwrap().lyrics.text().trim(), "found");
}

#[test]
fn editor_protects_drafts_and_ignores_stale_async_answers() {
    let mut app = app_fixture();
    let mut editor = crate::real::editor::Editor::new(
        2,
        muzon_core::editing::EditorSource {
            track: track(42),
            lyrics: "[00:00]Original".into(),
            lyrics_in_tag: false,
        },
    );
    editor.loading = false;
    app.editor = Some(editor);
    let _ = app.update(Message::EditorField(0, "New title".into()));
    let _ = app.update(Message::EditorClose);
    assert!(app.editor.as_ref().unwrap().discard);
    let _ = app.update(Message::QuitRequested(None));
    assert!(!app.closing);
    assert!(app.editor.is_some());
    let _ = app.update(Message::EditorLoaded(1, Err("stale".into())));
    assert_eq!(app.editor.as_ref().unwrap().fields[0], "New title");
    let _ = app.update(Message::EditorKeep);
    assert!(!app.editor.as_ref().unwrap().discard);
    let _ = crate::real::view::view(&app);
    let _ = app.update(Message::EditorTab(crate::real::editor::Tab::Lyrics));
    let _ = crate::real::view::view(&app);
    let editor = app.editor.as_mut().unwrap();
    editor.saving = true;
    let _ = app.update(Message::EditorDiscard);
    assert!(app.editor.is_some());
    let _ = app.update(Message::EditorSaved(
        2,
        crate::real::editor::Tab::Tags,
        Err("disk full".into()),
    ));
    assert!(app.editor.as_ref().unwrap().tags_dirty);
    assert_eq!(app.editor.as_ref().unwrap().fields[0], "New title");
    let _ = app.update(Message::EditorDiscard);
    assert!(app.editor.is_none());
}

#[test]
fn grouping_and_search_preserve_queue_and_resolve_original_tracks() {
    let mut app = app_fixture();
    let queue = app.snapshot.queue.clone();
    let _ = app.update(Message::ScreenSelected(Screen::Albums));
    assert_eq!(app.groups.len(), 1);
    let _ = app.update(Message::SearchChanged("99".into()));
    assert_eq!(app.filtered, vec![1]);
    assert_eq!(filtered_track_id(&app.snapshot, &app.filtered, 0), Some(99));
    assert!(Arc::ptr_eq(&queue, &app.snapshot.queue));
    let _ = app.update(Message::GroupSelected(usize::MAX));
    assert_eq!(app.filtered, vec![1]);
}

#[test]
fn search_rows_resolve_to_library_ids_not_row_numbers() {
    let mut state = Snapshot::empty();
    state.library = Arc::new(vec![track(42), track(99), track(7)]);
    assert_eq!(filtered_track_id(&state, &[2, 0], 0), Some(7));
    assert_eq!(filtered_track_id(&state, &[2, 0], 1), Some(42));
    assert_eq!(filtered_track_id(&state, &[2], 5), None);
    assert_eq!(filtered_track_id(&state, &[99], 0), None);
}
#[test]
fn duplicate_queue_entries_are_addressed_by_occurrence() {
    let mut state = Snapshot::empty();
    state.queue = Arc::new(vec![track(42), track(42), track(7)]);
    state.display = Arc::new(vec![1, 2, 0]);
    assert_eq!(displayed_queue_index(&state, 0), Some(1));
    assert_eq!(displayed_queue_index(&state, 2), Some(0));
    assert_eq!(displayed_queue_index(&state, 3), None);
}
#[test]
fn album_browsing_resolves_stable_keys_and_never_changes_queue() {
    let mut app = app_fixture();
    let queue = app.snapshot.queue.clone();
    let _ = app.update(Message::ScreenSelected(Screen::Albums));
    assert!(app.album_grid.browsing);
    let key = app.groups[0].key.clone();
    let _ = app.update(Message::AlbumSelected(key.clone()));
    assert!(!app.album_grid.browsing);
    assert_eq!(app.selected_group, Some(key.clone()));
    let _ = app.update(Message::AlbumSelected(("missing".into(), "missing".into())));
    assert_eq!(app.selected_group, Some(key));
    let _ = app.update(Message::AlbumsBack);
    assert!(app.album_grid.browsing);
    assert!(Arc::ptr_eq(&queue, &app.snapshot.queue));
    let _ = crate::real::album_grid::view(&app);
}
#[test]
fn folder_cancel_preserves_input_and_invalid_choice_reports_error() {
    let mut app = app_fixture();
    app.folder_input = "/keep".into();
    app.folder_picker_open = true;
    let _ = app.update(Message::FolderPicked(None));
    assert!(!app.folder_picker_open);
    assert_eq!(app.folder_input, "/keep");
    let _ = app.update(Message::FolderPicked(Some(
        "/does-not-exist/muzon-folder".into(),
    )));
    assert!(app.ui_error.is_some());
    assert_eq!(app.folder_input, "/keep");
}
#[test]
fn every_settings_category_can_build_and_keeps_playback_queue() {
    let mut app = app_fixture();
    let queue = app.snapshot.queue.clone();
    for (screen, _) in crate::real::settings::CATEGORIES {
        let _ = app.update(Message::ScreenSelected(screen));
        let _ = crate::real::settings::view(&app);
        assert!(Arc::ptr_eq(&queue, &app.snapshot.queue));
    }
}

#[test]
fn quit_waits_for_service_cancellation_and_freezes_new_drafts() {
    use crate::real::services::{Action, Reply};
    let mut app = app_fixture();
    let cancel = muzon_services::hub::CancellationToken::new();
    app.services.busy = Some(Action::Sync);
    app.services.generation = 4;
    app.services.cancel = Some(cancel.clone());
    let _ = app.update(Message::QuitRequested(None));
    assert!(cancel.is_cancelled());
    assert!(app.services.pending_quit);
    assert!(!app.closing, "service must finish before shutdown starts");
    let original = app.appearance.accent_input.clone();
    let _ = app.update(Message::AppearanceAccentInput("#123456".into()));
    let _ = app.update(Message::SeekChanged(50.));
    let _ = app.update(Message::NewPlaylistName("late draft".into()));
    assert_eq!(app.appearance.accent_input, original);
    assert!(app.seek_preview.is_none());
    assert!(app.new_playlist_name.is_empty());
    let _ = app.update(Message::ServiceFinished(
        3,
        Action::Sync,
        Ok(Reply::Cancelled),
    ));
    assert!(
        app.services.pending_quit,
        "stale replies cannot release shutdown"
    );
    assert!(app.services.busy.is_some());
    let _ = app.update(Message::ServiceFinished(
        4,
        Action::Sync,
        Ok(Reply::Cancelled),
    ));
    assert!(!app.services.pending_quit);
    assert!(app.services.busy.is_none());
}

#[test]
fn discarding_audio_on_quit_still_protects_editor_drafts() {
    let mut app = app_fixture();
    let mut editor = crate::real::editor::Editor::new(
        1,
        muzon_core::editing::EditorSource {
            track: track(42),
            lyrics: "unsaved lyrics".into(),
            lyrics_in_tag: false,
        },
    );
    editor.loading = false;
    editor.lyrics_dirty = true;
    app.editor = Some(editor);
    let _ = app.update(Message::DiscardAudioAndQuit);
    assert!(!app.closing);
    assert!(app.editor.as_ref().unwrap().discard);
}
