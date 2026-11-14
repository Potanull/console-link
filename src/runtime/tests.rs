use super::*;
fn resolution_runtime() -> Runtime {
    let mut r = Runtime::new(false);
    r.settings.video_id = "card".into();
    r.devices.video.push(VideoDevice {
        id: "card".into(),
        name: "Card".into(),
        modes: vec![
            capture::VideoMode {
                width: 1920,
                height: 1080,
                fps_num: 30,
                fps_den: 1,
            },
            capture::VideoMode {
                width: 1920,
                height: 1080,
                fps_num: 50,
                fps_den: 1,
            },
            capture::VideoMode {
                width: 1280,
                height: 720,
                fps_num: 60000,
                fps_den: 1001,
            },
            capture::VideoMode {
                width: 1280,
                height: 720,
                fps_num: 30000,
                fps_den: 1001,
            },
        ],
    });
    r
}
#[test]
fn resolution_choice_uses_advertised_rate_and_fps_changes_keep_resolution() {
    let mut r = resolution_runtime();
    r.select_resolution(1920, 1080);
    assert!(!r.settings.capture_preferences.automatic);
    assert_eq!(r.settings.mode.as_ref().unwrap().fps_num, 50);
    r.settings.capture_preferences.preferred_fps = 30;
    r.changed();
    assert_eq!(r.settings.mode.as_ref().unwrap().fps_num, 30);
    r.settings.capture_preferences.preferred_fps = 60;
    r.changed();
    let mode = r.settings.mode.as_ref().unwrap();
    assert_eq!(
        (mode.width, mode.height, mode.fps_num, mode.fps_den),
        (1920, 1080, 50, 1)
    );
    assert!(r.devices.video[0].modes.contains(mode));
    r.select_resolution(3840, 2160);
    assert!(r.settings.mode.is_none());
}
#[test]
fn manual_resolution_target_changes_preserve_native_fraction() {
    let mut r = resolution_runtime();
    r.select_resolution(1280, 720);
    let mode = r.settings.mode.as_ref().unwrap();
    assert_eq!((mode.fps_num, mode.fps_den), (60000, 1001));
    r.settings.capture_preferences.preferred_fps = 30;
    r.changed();
    let mode = r.settings.mode.as_ref().unwrap();
    assert_eq!(
        (mode.width, mode.height, mode.fps_num, mode.fps_den),
        (1280, 720, 30000, 1001)
    );
    assert!(r.devices.video[0].modes.contains(mode));
}
#[test]
fn ten_ms_polls_keep_sixty_fps_phase_and_skip_stalled_slots() {
    let mut deadline = None;
    let mut count = 0;
    for ms in (0..1000u128).step_by(10) {
        let now = ms * 1_000_000 * 60;
        if deadline.is_none_or(|d| now >= d) {
            count += 1;
            deadline = Some(next_deadline(deadline, now));
        }
    }
    assert_eq!(count, 60);
    let stalled = 10_000_000_000u128 * 60;
    assert!(next_deadline(deadline, stalled) > stalled);
}
#[test]
fn isolated_runtime_and_late_capture_events_do_not_restart_session() {
    let mut r = Runtime::new(false);
    assert!(r.store.is_none() && r.auth.is_none() && r.discord.is_none());
    r.cancelled = Arc::new(AtomicBool::new(false));
    r.generation = 7;
    r.opening = true;
    r.stop();
    r.tx.send(Event::Started(7, Err("late error".into())))
        .unwrap();
    r.frames_tx
        .send((
            7,
            VideoFrame {
                width: 1,
                height: 1,
                rgba: vec![0; 4],
                timestamp_ns: monotonic_ns(),
            },
        ))
        .unwrap();
    assert!(r.poll().is_none());
    assert!(!r.active() && !r.opening);
    assert!(r.error.is_none());
    assert_eq!(r.history.bytes(), 0);
}
#[test]
fn presence_requires_account_and_current_game_and_obeys_disable() {
    let mut r = Runtime::new(false);
    r.settings.discord_enabled = true;
    r.settings.nintendo_enabled = true;
    r.nintendo.game = Some("Current game".into());
    r.nintendo.started_at = 123;
    assert!(r.presence(false).is_none());
    let p = r.presence(true).unwrap();
    assert_eq!(p.game, "Current game");
    assert_eq!(p.started_at, 123);
    r.nintendo.game = None;
    assert!(r.presence(true).is_none());
    r.settings.discord_enabled = false;
    assert!(r.presence(true).is_none());
}
#[test]
fn choosing_same_game_preserves_timer() {
    let mut r = Runtime::new(false);
    r.choose_game("Test".into());
    r.started_unix = 123;
    r.choose_game("Test".into());
    assert_eq!(r.game_started_at(), 123);
    r.choose_game("Other".into());
    assert_ne!(r.game_started_at(), 123);
}
#[test]
fn same_title_manual_override_keeps_nintendo_elapsed_time() {
    let mut r = Runtime::new(false);
    r.settings.nintendo_enabled = true;
    r.nintendo.game = Some("Current game".into());
    r.nintendo.started_at = 123;
    r.choose_game("Current game".into());
    assert!(!r.settings.nintendo_enabled);
    assert_eq!(r.game_started_at(), 123);
}
#[test]
fn close_reaps_queued_native_session_off_ui_thread() {
    struct BlockedDrop {
        entered: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    }
    impl CaptureSession for BlockedDrop {}
    impl Drop for BlockedDrop {
        fn drop(&mut self) {
            let _ = self.entered.send(());
            let _ = self.release.recv();
        }
    }
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let mut r = Runtime::new(false);
    r.tx.send(Event::Started(
        0,
        Ok(Active {
            video: Box::new(BlockedDrop {
                entered: entered_tx,
                release: release_rx,
            }),
            audio: None,
            cancelled: Arc::new(AtomicBool::new(false)),
        }),
    ))
    .unwrap();
    r.close();
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    release_tx.send(()).unwrap();
}
#[test]
fn screenshot_roundtrip_preserves_alpha_and_srgb_metadata() {
    let directory =
        std::env::temp_dir().join(format!("console-link-runtime-png-{}", std::process::id()));
    let frame = VideoFrame {
        width: 2,
        height: 1,
        rgba: vec![25, 100, 200, 0, 255, 80, 0, 137],
        timestamp_ns: 0,
    };
    let path = save_screenshot(&frame, &directory).unwrap();
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    assert_eq!(
        reader.info().srgb,
        Some(png::SrgbRenderingIntent::Perceptual)
    );
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(&bytes[..info.buffer_size()], &frame.rgba);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(directory).unwrap();
}
#[test]
fn screenshot_rejects_invalid_frame_before_creating_directory() {
    let path =
        std::env::temp_dir().join(format!("console-link-invalid-frame-{}", std::process::id()));
    assert!(
        save_screenshot(
            &VideoFrame {
                width: 2,
                height: 2,
                rgba: vec![0; 4],
                timestamp_ns: 0
            },
            &path
        )
        .is_err()
    );
    assert!(!path.exists());
}

#[test]
fn video_only_ignores_unavailable_audio_without_losing_selections() {
    let mut runtime = resolution_runtime();
    runtime.select_resolution(1920, 1080);
    runtime.settings.audio_input_id = "unavailable-input".into();
    runtime.settings.audio_output_id = "unavailable-output".into();
    runtime.refreshing_audio = true;
    assert!(!runtime.ready());
    runtime.settings.audio_enabled = false;
    assert!(runtime.ready());
    runtime.settings.audio_enabled = true;
    assert!(!runtime.ready());
    assert_eq!(runtime.settings.audio_input_id, "unavailable-input");
}

#[test]
fn high_refresh_pacing_preserves_120_real_frame_slots() {
    let mut deadline = None;
    let mut count = 0;
    for ms in (0..1000u128).step_by(2) {
        let now = ms * 1_000_000 * 120;
        if deadline.is_none_or(|d| now >= d) {
            count += 1;
            deadline = Some(next_deadline(deadline, now));
        }
    }
    assert_eq!(count, 120);
}
