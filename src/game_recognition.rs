//! Optional local title-screen OCR. Capture frames never leave the computer.
use crate::types::VideoFrame;
use crossbeam_channel::{Sender, bounded};
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Game {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub cover_url: Option<String>,
    pub game_url: Option<String>,
}

pub fn catalog() -> &'static [Game] {
    static GAMES: OnceLock<Vec<Game>> = OnceLock::new();
    GAMES.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/games.json"))
            .expect("bundled game catalog must be valid")
    })
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '™' | '®' | '©'))
        .collect::<String>()
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

struct IndexedGame {
    aliases: Vec<String>,
}
fn index() -> &'static [IndexedGame] {
    static INDEX: OnceLock<Vec<IndexedGame>> = OnceLock::new();
    INDEX.get_or_init(|| {
        catalog()
            .iter()
            .map(|game| IndexedGame {
                aliases: std::iter::once(&game.name)
                    .chain(&game.aliases)
                    .map(|name| normalize(name))
                    .collect(),
            })
            .collect()
    })
}

pub fn find_game(name: &str) -> Option<Game> {
    let name = normalize(name);
    if name.is_empty() {
        return None;
    }
    index()
        .iter()
        .position(|entry| entry.aliases.contains(&name))
        .map(|i| catalog()[i].clone())
}

pub fn search_games(query: &str, limit: usize) -> Vec<Game> {
    let query = normalize(query);
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut results: Vec<_> = index()
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            entry
                .aliases
                .iter()
                .filter(|name| name.contains(&query))
                .map(|name| {
                    if *name == query {
                        0
                    } else if name.starts_with(&query) {
                        1
                    } else {
                        2
                    }
                })
                .min()
                .map(|rank| (rank, i))
        })
        .collect();
    results.sort_by_key(|&(rank, i)| (rank, &catalog()[i].name));
    results
        .into_iter()
        .take(limit.min(100))
        .map(|(_, i)| catalog()[i].clone())
        .collect()
}

fn match_text(text: &str) -> Option<(Game, f32)> {
    let text = format!(" {} ", normalize(text));
    let mut candidates: Vec<(usize, usize, &str)> = index()
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            let alias = entry
                .aliases
                .iter()
                .filter(|alias| {
                    // A full title must be visible. Franchise fragments and short words
                    // such as "Go", "Inside" or "Zelda" are not reliable evidence.
                    let words = alias.split_whitespace().count();
                    alias.chars().count() >= 8
                        && (words >= 2 || alias.chars().count() >= 9)
                        && ![
                            "press any button",
                            "press any key",
                            "new game",
                            "select game",
                            "main menu",
                            "start game",
                        ]
                        .contains(&alias.as_str())
                        && text.contains(&format!(" {alias} "))
                })
                .max_by_key(|alias| alias.len())?;
            Some((alias.len(), i, alias.as_str()))
        })
        .collect();
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
    let &(_, i, best_alias) = candidates.first()?;
    // A longer title may include another complete game's name (Minecraft
    // Dungeons). Two unrelated complete titles remain ambiguous.
    if candidates
        .iter()
        .skip(1)
        .any(|&(_, _, alias)| !format!(" {best_alias} ").contains(&format!(" {alias} ")))
    {
        return None;
    }
    Some((catalog()[i].clone(), 1.0))
}

#[derive(Clone, Debug)]
pub struct RecognitionSuggestion {
    pub session_generation: u64,
    pub timestamp_ns: u64,
    pub game: Game,
    pub confidence: f32,
}

struct Job {
    epoch: u64,
    session: u64,
    frame: Arc<VideoFrame>,
}
struct State {
    enabled: bool,
    stopped: bool,
    epoch: u64,
    session: u64,
    last_submit: Option<Instant>,
    pending: Option<Job>,
    output: Option<RecognitionSuggestion>,
    status: String,
}

pub struct RecognitionWorker {
    state: Arc<Mutex<State>>,
    wake: Sender<()>,
}
impl RecognitionWorker {
    pub fn new(cache_path: PathBuf) -> Self {
        let state = Arc::new(Mutex::new(State {
            enabled: false,
            stopped: false,
            epoch: 0,
            session: 0,
            last_submit: None,
            pending: None,
            output: None,
            status: "Распознавание выключено".into(),
        }));
        let (wake, receiver) = bounded(1);
        let shared = state.clone();
        std::thread::spawn(move || {
            let mut engine = None;
            let mut confirmation = Confirmation::default();
            while receiver.recv().is_ok() {
                let job = {
                    let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
                    if state.stopped {
                        break;
                    }
                    if !state.enabled {
                        engine = None;
                        confirmation = Confirmation::default();
                        continue;
                    }
                    state.pending.take()
                };
                let Some(job) = job else {
                    continue;
                };
                let result = (|| -> Result<Option<(Game, f32)>, String> {
                    if engine.is_none() {
                        engine = Some(load_engine(
                            &cache_path,
                            || current(&shared, &job),
                            |message| status(&shared, &job, message),
                        )?);
                    }
                    if !current(&shared, &job) {
                        return Err("Отменено".into());
                    }
                    status(&shared, &job, "Ищу название игры на экране...");
                    let text = recognize(engine.as_ref().expect("loaded engine"), &job.frame)?;
                    Ok(match_text(&text))
                })();
                if !current(&shared, &job) {
                    confirmation = Confirmation::default();
                    continue;
                }
                match result {
                    Ok(candidate) => {
                        if let Some((game, confidence)) = candidate {
                            if confirmation.observe(
                                job.epoch,
                                job.session,
                                job.frame.timestamp_ns,
                                &game.id,
                            ) {
                                let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
                                if state.enabled
                                    && state.epoch == job.epoch
                                    && state.session == job.session
                                {
                                    state.status = format!("Распознано: {}", game.name);
                                    state.output = Some(RecognitionSuggestion {
                                        session_generation: job.session,
                                        timestamp_ns: job.frame.timestamp_ns,
                                        game,
                                        confidence,
                                    });
                                }
                            } else {
                                status(&shared, &job, "Проверяю название повторно...");
                            }
                        } else {
                            confirmation = Confirmation::default();
                            status(
                                &shared,
                                &job,
                                "Название не найдено. Открой титульный экран или выбери игру вручную.",
                            );
                        }
                    }
                    Err(error) => status(&shared, &job, &format!("Распознавание: {error}")),
                }
            }
        });
        Self { state, wake }
    }
    pub fn set_enabled(&self, enabled: bool) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.enabled == enabled {
            return;
        }
        state.enabled = enabled;
        state.epoch = state.epoch.wrapping_add(1);
        state.pending = None;
        state.output = None;
        state.last_submit = None;
        state.status = if enabled {
            "Ожидаю кадр консоли. Первый запуск загрузит OCR-модели (12 МБ)."
        } else {
            "Распознавание выключено"
        }
        .into();
        let _ = self.wake.try_send(());
    }
    pub fn submit(&self, session_generation: u64, frame: Arc<VideoFrame>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.enabled || state.stopped {
            return;
        }
        if state.session != session_generation {
            state.session = session_generation;
            state.last_submit = None;
            state.pending = None;
            state.output = None;
        }
        if state
            .last_submit
            .is_some_and(|last| last.elapsed() < Duration::from_secs(4))
        {
            return;
        }
        state.last_submit = Some(Instant::now());
        state.pending = Some(Job {
            epoch: state.epoch,
            session: session_generation,
            frame,
        });
        let _ = self.wake.try_send(());
    }
    pub fn status(&self) -> String {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status
            .clone()
    }
    pub fn poll(&self) -> Option<RecognitionSuggestion> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let output = state.output.take()?;
        (state.enabled && output.session_generation == state.session).then_some(output)
    }
}
impl Drop for RecognitionWorker {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.stopped = true;
        state.enabled = false;
        state.pending = None;
        let _ = self.wake.try_send(());
    }
}
fn current(shared: &Mutex<State>, job: &Job) -> bool {
    let state = shared.lock().unwrap_or_else(|e| e.into_inner());
    state.enabled && !state.stopped && state.epoch == job.epoch && state.session == job.session
}
fn status(shared: &Mutex<State>, job: &Job, message: &str) {
    let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
    if state.enabled && state.epoch == job.epoch && state.session == job.session {
        state.status = message.into();
    }
}

#[derive(Default)]
struct Confirmation {
    last: Option<(u64, u64, u64, String)>,
}
impl Confirmation {
    fn observe(&mut self, epoch: u64, session: u64, timestamp: u64, id: &str) -> bool {
        let confirmed = self.last.as_ref().is_some_and(|(e, s, t, game)| {
            *e == epoch && *s == session && timestamp > *t && game == id
        });
        self.last = Some((epoch, session, timestamp, id.into()));
        confirmed
    }
}

struct ModelFile {
    name: &'static str,
    bytes: u64,
    sha256: &'static str,
}
const MODELS: [ModelFile; 2] = [
    ModelFile {
        name: "text-detection.onnx",
        bytes: 2_499_479,
        sha256: "a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe",
    },
    ModelFile {
        name: "text-recognition.onnx",
        bytes: 9_713_177,
        sha256: "86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5",
    },
];

fn verified_file(path: &Path, model: &ModelFile) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    if file.metadata().map(|m| m.len()).ok() != Some(model.bytes) {
        return false;
    }
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash).is_ok() && format!("{:x}", hash.finalize()) == model.sha256
}

fn load_engine(
    cache: &Path,
    active: impl Fn() -> bool,
    progress: impl Fn(&str),
) -> Result<OcrEngine, String> {
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    for model in &MODELS {
        let destination = cache.join(model.name);
        if verified_file(&destination, model) {
            continue;
        }
        if !active() {
            return Err("Отменено".into());
        }
        progress("Загружаю OCR-модели (12 МБ). Кадры никуда не отправляются.");
        let temporary = destination.with_extension("part");
        let result = (|| -> Result<(), String> {
            let client = oauth2::reqwest::blocking::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(20))
                .redirect(oauth2::reqwest::redirect::Policy::custom(|attempt| {
                    if attempt.previous().len() >= 5 || attempt.url().scheme() != "https" {
                        attempt.error("Недопустимое перенаправление OCR-модели")
                    } else {
                        attempt.follow()
                    }
                }))
                .build()
                .map_err(|_| "Не удалось создать клиент загрузки".to_string())?;
            let url = format!(
                "https://ocrs-models.s3-accelerate.amazonaws.com/{}",
                model.name
            );
            if !active() {
                return Err("Отменено".into());
            }
            let mut response = client
                .get(url)
                .send()
                .and_then(|r| r.error_for_status())
                .map_err(|error| {
                    if error.is_timeout() {
                        "Истекло время загрузки OCR-модели. Повторю на следующем кадре.".to_string()
                    } else if let Some(status) = error.status() {
                        format!("Сервер OCR-моделей вернул HTTP {status}")
                    } else {
                        "Не удалось загрузить OCR-модель. Проверь подключение к интернету."
                            .to_string()
                    }
                })?;
            if !active() {
                return Err("Отменено".into());
            }
            let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
            let mut total = 0_u64;
            let mut buffer = [0; 16 * 1024];
            loop {
                if !active() {
                    return Err("Отменено".into());
                }
                let count = response
                    .read(&mut buffer)
                    .map_err(|_| "Ошибка чтения OCR-модели".to_string())?;
                if !active() {
                    return Err("Отменено".into());
                }
                if count == 0 {
                    break;
                }
                total += count as u64;
                if total > model.bytes {
                    return Err("Неверный размер OCR-модели".into());
                }
                file.write_all(&buffer[..count])
                    .map_err(|e| e.to_string())?;
            }
            file.flush().map_err(|e| e.to_string())?;
            drop(file);
            if !active() {
                return Err("Отменено".into());
            }
            if !verified_file(&temporary, model) {
                return Err("Контрольная сумма OCR-модели не совпала".into());
            }
            // A corrupt cached file may exist on Windows, where rename cannot replace it.
            if destination.exists() {
                std::fs::remove_file(&destination).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&temporary, &destination).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result?;
    }
    if !active() {
        return Err("Отменено".into());
    }
    progress("Подготавливаю локальное распознавание...");
    let detection =
        rten::Model::load_file(cache.join(MODELS[0].name)).map_err(|e| e.to_string())?;
    let recognition =
        rten::Model::load_file(cache.join(MODELS[1].name)).map_err(|e| e.to_string())?;
    OcrEngine::new(OcrEngineParams {
        detection_model: Some(detection),
        recognition_model: Some(recognition),
        ..Default::default()
    })
    .map_err(|e| e.to_string())
}

fn recognize(engine: &OcrEngine, frame: &VideoFrame) -> Result<String, String> {
    let pixels = u64::from(frame.width) * u64::from(frame.height);
    if pixels == 0 || pixels > 16_777_216 || pixels * 4 != frame.rgba.len() as u64 {
        return Err("Некорректный кадр".into());
    }
    let input = image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(
        frame.width,
        frame.height,
        &frame.rgba,
    )
    .ok_or("Некорректный кадр")?;
    let factor = (1280.0 / f64::from(frame.width.max(frame.height))).min(1.0);
    let width = (f64::from(frame.width) * factor).round().max(1.0) as u32;
    let height = (f64::from(frame.height) * factor).round().max(1.0) as u32;
    let resized =
        image::imageops::resize(&input, width, height, image::imageops::FilterType::Triangle);
    let rgb: Vec<u8> = resized.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let source = ImageSource::from_bytes(&rgb, (width, height)).map_err(|e| e.to_string())?;
    let input = engine.prepare_input(source).map_err(|e| e.to_string())?;
    engine.get_text(&input).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_catalog_supports_titles_beyond_a_manual_shortlist() {
        assert!(catalog().len() > 20_000);
        let ids: std::collections::HashSet<_> = catalog().iter().map(|g| &g.id).collect();
        assert_eq!(ids.len(), catalog().len());
        assert!(find_game("The Legend of Zelda: Breath of the Wild").is_some());
        assert!(find_game("Mario Kart 8 Deluxe").is_some());
        assert!(find_game("Blaster Master Zero").is_some());
        assert!(search_games("blaster master", 4).len() >= 3);
        assert!(find_game("").is_none());
    }

    #[test]
    fn matching_requires_a_full_unambiguous_title() {
        let found = match_text("THE LEGEND OF ZELDA\nBREATH OF THE WILD\nPress A")
            .unwrap()
            .0;
        assert_eq!(
            find_game("The Legend of Zelda: Breath of the Wild")
                .unwrap()
                .id,
            found.id
        );
        assert!(match_text("Zelda\nPokemon\nPress any button\nNew Game").is_none());
        assert!(
            match_text("The Legend of Zelda Breath of the Wild\nMario Kart 8 Deluxe").is_none()
        );
        assert!(match_text("Random dialog, health 100, credits, continue").is_none());
        assert_eq!(normalize("Pokémon™: Let’s Go®"), "pokemon let s go");
    }

    #[test]
    fn confirmation_requires_distinct_frames_in_the_same_enabled_session() {
        let mut confirmation = Confirmation::default();
        assert!(!confirmation.observe(1, 8, 100, "zelda"));
        assert!(!confirmation.observe(1, 8, 100, "zelda"));
        assert!(confirmation.observe(1, 8, 200, "zelda"));
        assert!(!confirmation.observe(1, 9, 300, "zelda"));
        assert!(!confirmation.observe(2, 9, 400, "zelda"));
        assert!(!confirmation.observe(2, 9, 500, "mario"));
        assert!(confirmation.observe(2, 9, 600, "mario"));
    }

    #[test]
    fn disabled_worker_does_not_queue_frames_or_create_model_cache() {
        let cache = std::env::temp_dir().join(format!(
            "console-link-disabled-{}",
            crate::types::monotonic_ns()
        ));
        let worker = RecognitionWorker::new(cache.clone());
        worker.submit(
            1,
            Arc::new(VideoFrame {
                width: 1,
                height: 1,
                rgba: vec![0; 4],
                timestamp_ns: 1,
            }),
        );
        assert!(worker.state.lock().unwrap().pending.is_none());
        assert!(worker.poll().is_none());
        assert!(!cache.exists());
        {
            let mut state = worker.state.lock().unwrap();
            state.enabled = true;
            state.pending = Some(Job {
                epoch: 0,
                session: 1,
                frame: Arc::new(VideoFrame {
                    width: 1,
                    height: 1,
                    rgba: vec![0; 4],
                    timestamp_ns: 1,
                }),
            });
        }
        worker.set_enabled(false);
        assert!(worker.state.lock().unwrap().pending.is_none());
        assert!(!cache.exists());
    }

    #[test]
    fn checksum_rejects_corrupt_or_incomplete_model_files() {
        let path = std::env::temp_dir().join(format!(
            "console-link-model-check-{}",
            crate::types::monotonic_ns()
        ));
        std::fs::write(&path, b"not an OCR model").unwrap();
        assert!(!verified_file(&path, &MODELS[0]));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn poll_discards_a_result_from_a_previous_session() {
        let worker =
            RecognitionWorker::new(std::env::temp_dir().join("console-link-unused-model-cache"));
        {
            let mut state = worker.state.lock().unwrap();
            state.enabled = true;
            state.session = 2;
            state.output = Some(RecognitionSuggestion {
                session_generation: 1,
                timestamp_ns: 42,
                game: catalog()[0].clone(),
                confidence: 1.0,
            });
        }
        assert!(worker.poll().is_none());
        worker.set_enabled(false);
    }

    #[test]
    #[ignore = "loads pinned local models; set CONSOLE_LINK_OCR_TEST_CACHE, run in release mode"]
    fn real_ocr_recognizes_a_rendered_title_screen() {
        use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
        let cache = PathBuf::from(
            std::env::var_os("CONSOLE_LINK_OCR_TEST_CACHE").expect("explicit test cache required"),
        );
        let engine = load_engine(&cache, || true, |message| eprintln!("{message}")).unwrap();
        let font =
            FontArc::try_from_slice(include_bytes!("../assets/fonts/InterVariable.ttf")).unwrap();
        let scaled = font.as_scaled(PxScale::from(56.0));
        let mut picture =
            image::RgbaImage::from_pixel(1100, 400, image::Rgba([255, 255, 255, 255]));
        for (line, baseline) in [
            ("The Legend of Zelda", 140.0),
            ("Breath of the Wild", 235.0),
        ] {
            let mut cursor = 80.0;
            for character in line.chars() {
                let id = scaled.glyph_id(character);
                let glyph = id.with_scale_and_position(scaled.scale(), point(cursor, baseline));
                if let Some(outline) = font.outline_glyph(glyph) {
                    let bounds = outline.px_bounds();
                    outline.draw(|x, y, coverage| {
                        let px = bounds.min.x as i32 + x as i32;
                        let py = bounds.min.y as i32 + y as i32;
                        if px >= 0 && py >= 0 && px < 1100 && py < 400 {
                            let shade = (255.0 * (1.0 - coverage)) as u8;
                            picture.put_pixel(
                                px as u32,
                                py as u32,
                                image::Rgba([shade, shade, shade, 255]),
                            );
                        }
                    });
                }
                cursor += scaled.h_advance(id);
            }
        }
        picture
            .save(cache.join("synthetic-title-test.png"))
            .unwrap();
        let frame = VideoFrame {
            width: 1100,
            height: 400,
            rgba: picture.into_raw(),
            timestamp_ns: 1,
        };
        let text = recognize(&engine, &frame).unwrap();
        eprintln!("Synthetic title OCR: {text}");
        let actual = match_text(&text)
            .expect("real OCR must identify the rendered title")
            .0;
        assert_eq!(
            actual.id,
            find_game("The Legend of Zelda Breath of the Wild")
                .unwrap()
                .id
        );
    }
}
