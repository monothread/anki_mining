//! `anki-mining --self-test [resources dir] [output file]`: checks, without opening a window, that
//! everything the app needs is there and works on this machine: the bundled dictionary, the
//! tokenizer, the OCR (models and ONNX Runtime), the voice, the screen capture and the pointer.
//! It prints one line per check, writes the same lines to the output file (a Windows program
//! without a console has nowhere to print) and exits with 0 when every check passed.

use std::path::{Path, PathBuf};

use crate::capture::{Region, ScreenCapture, SystemCapture};
use crate::dictionary::{install_bundled, Dictionary};
use crate::ocr::{lines_to_text, MeikiEngine, OcrEngine};
use crate::pointer::{PointerTracker, SystemPointer};
use crate::tokenizer::Tokenizer;
use crate::tts::{Speaker, VoicevoxSpeaker, DEFAULT_STYLE_ID};

/// A small screenshot of a subtitle and what the OCR must read from it.
const SUBTITLE_IMAGE: &[u8] =
    include_bytes!("../../tests/fixtures/ocr/07_720p_thin_dark__half_res.png");
const SUBTITLE_TEXT: &str = "明日の朝までに終わらせないと";

/// The bundled dictionary has about 219,000 entries; far fewer means a broken file.
const MIN_ENTRIES: usize = 200_000;

/// The outcome of one check: what it found, or why it failed.
type Check = Result<String, String>;

/// What the command line asked for.
#[derive(Debug, PartialEq, Eq)]
pub struct SelfTestRequest {
    pub resources: Option<PathBuf>,
    pub output: Option<PathBuf>,
}

/// The self-test request in the arguments (the program name first), if there is one.
pub fn parse_arguments(arguments: &[String]) -> Option<SelfTestRequest> {
    let position = arguments
        .iter()
        .position(|argument| argument == "--self-test")?;
    let mut rest = arguments[position + 1..].iter();
    Some(SelfTestRequest {
        resources: rest.next().map(PathBuf::from),
        output: rest.next().map(PathBuf::from),
    })
}

/// Where the bundled files are when the app is installed: next to the executable, or in a
/// `resources` folder there.
pub fn find_resources(executable_directory: &Path) -> Option<PathBuf> {
    [
        executable_directory.to_path_buf(),
        executable_directory.join("resources"),
    ]
    .into_iter()
    .find(|candidate| candidate.join("ocr").is_dir())
}

/// Runs the self-test when the arguments ask for it and returns the exit code; `None` when they
/// do not (the app starts normally).
pub fn run_if_requested(arguments: &[String]) -> Option<i32> {
    let request = parse_arguments(arguments)?;
    let resources = request.resources.clone().or_else(|| {
        let executable = std::env::current_exe().ok()?;
        find_resources(executable.parent()?)
    });
    let report = match &resources {
        Some(directory) => run_checks(directory),
        None => vec![(
            "resources".to_string(),
            Err("cannot find the bundled files (ocr, dictionary); pass their folder".to_string()),
        )],
    };
    let (text, passed) = format_report(&report);
    print!("{text}");
    if let Some(output) = &request.output {
        let _ = std::fs::write(output, &text);
    }
    Some(if passed { 0 } else { 1 })
}

fn run_checks(resources: &Path) -> Vec<(String, Check)> {
    let mut report = vec![
        ("dictionary".to_string(), check_dictionary(resources)),
        ("tokenizer".to_string(), check_tokenizer()),
        ("ocr".to_string(), check_ocr(resources)),
        ("screen capture".to_string(), check_capture()),
        ("pointer".to_string(), check_pointer()),
    ];
    if resources.join("tts").is_dir() {
        report.push(("voice".to_string(), check_voice(&resources.join("tts"))));
    }
    report
}

/// One line per check (`PASS name: detail` or `FAIL name: reason`) and whether all of them passed.
pub fn format_report(report: &[(String, Check)]) -> (String, bool) {
    let mut text = String::new();
    let mut passed = true;
    for (name, outcome) in report {
        match outcome {
            Ok(detail) => text.push_str(&format!("PASS {name}: {detail}\n")),
            Err(reason) => {
                passed = false;
                text.push_str(&format!("FAIL {name}: {reason}\n"));
            }
        }
    }
    text.push_str(if passed {
        "SELF-TEST PASSED\n"
    } else {
        "SELF-TEST FAILED\n"
    });
    (text, passed)
}

fn check_dictionary(resources: &Path) -> Check {
    let bundled = resources.join("dictionary").join("jmdict.db");
    if !bundled.is_file() {
        return Err(format!("{} is missing", bundled.display()));
    }
    let directory =
        std::env::temp_dir().join(format!("anki-mining-selftest-{}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let installed = directory.join("jmdict.db");
    let outcome = (|| -> Check {
        install_bundled(&bundled, &installed).map_err(|error| error.to_string())?;
        let dictionary = Dictionary::open(&installed).map_err(|error| error.to_string())?;
        let entries = dictionary
            .entry_count()
            .map_err(|error| error.to_string())?;
        if entries < MIN_ENTRIES {
            return Err(format!("only {entries} entries"));
        }
        let found = dictionary
            .scan("日本語を勉強")
            .map_err(|error| error.to_string())?;
        let first = found.first().map(|result| result.source.as_str());
        if first != Some("日本語") {
            return Err(format!("scanning 日本語を勉強 found {first:?} first"));
        }
        let cat = dictionary.lookup("猫").map_err(|error| error.to_string())?;
        let pitch = cat
            .first()
            .map(|entry| entry.pitch.clone())
            .unwrap_or_default();
        if pitch.is_empty() {
            return Err("猫 has no pitch accent".to_string());
        }
        Ok(format!(
            "{entries} entries, 日本語 found, pitch of 猫 {pitch:?}"
        ))
    })();
    let _ = std::fs::remove_dir_all(&directory);
    outcome
}

fn check_tokenizer() -> Check {
    let tokenizer = Tokenizer::new().map_err(|error| error.to_string())?;
    let tokens = tokenizer
        .tokenize("日本語を勉強する")
        .map_err(|error| error.to_string())?;
    let words: Vec<&str> = tokens.iter().map(|token| token.surface.as_str()).collect();
    // UniDic splits 日本語 into 日本 and 語; the dictionary scan puts them back together.
    if words != ["日本", "語", "を", "勉強", "する"] {
        return Err(format!("the words were {words:?}"));
    }
    Ok(format!("{words:?}"))
}

fn check_ocr(resources: &Path) -> Check {
    let mut engine =
        MeikiEngine::from_directory(&resources.join("ocr")).map_err(|error| error.to_string())?;
    let image = image::load_from_memory(SUBTITLE_IMAGE)
        .map_err(|error| error.to_string())?
        .to_rgb8();
    let lines = engine
        .recognize(&image)
        .map_err(|error| error.to_string())?;
    let text = lines_to_text(&lines);
    if text.trim() != SUBTITLE_TEXT {
        return Err(format!("read {text:?} instead of {SUBTITLE_TEXT:?}"));
    }
    Ok(format!("read {text:?}"))
}

fn check_capture() -> Check {
    let mut capture = SystemCapture::connect().map_err(|error| error.to_string())?;
    let (width, height) = capture.screen_size();
    let image = capture
        .capture(Region {
            x: 0,
            y: 0,
            width: width.min(32),
            height: height.min(32),
        })
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "screen {width}x{height}, captured {}x{}",
        image.width(),
        image.height()
    ))
}

fn check_pointer() -> Check {
    let mut pointer = SystemPointer::connect().map_err(|error| error.to_string())?;
    let reading = pointer.read().map_err(|error| error.to_string())?;
    Ok(format!("at ({}, {})", reading.x, reading.y))
}

fn check_voice(directory: &Path) -> Check {
    let speaker =
        VoicevoxSpeaker::new(directory, DEFAULT_STYLE_ID).map_err(|error| error.to_string())?;
    let wav = speaker
        .speak("こんにちは")
        .map_err(|error| error.to_string())?;
    if !wav.starts_with(b"RIFF") {
        return Err("the sound is not a WAV".to_string());
    }
    Ok(format!("{} bytes of speech", wav.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn the_flag_alone_asks_for_a_self_test_with_the_defaults() {
        assert_eq!(
            parse_arguments(&arguments(&["anki-mining", "--self-test"])),
            Some(SelfTestRequest {
                resources: None,
                output: None
            })
        );
    }

    #[test]
    fn the_resources_folder_and_the_output_file_follow_the_flag() {
        assert_eq!(
            parse_arguments(&arguments(&[
                "anki-mining",
                "--self-test",
                "res",
                "out.txt",
                "extra"
            ])),
            Some(SelfTestRequest {
                resources: Some(PathBuf::from("res")),
                output: Some(PathBuf::from("out.txt"))
            })
        );
    }

    #[test]
    fn without_the_flag_the_app_starts_normally() {
        assert_eq!(parse_arguments(&arguments(&["anki-mining"])), None);
        assert_eq!(parse_arguments(&arguments(&["anki-mining", "res"])), None);
        assert_eq!(run_if_requested(&arguments(&["anki-mining"])), None);
    }

    #[test]
    fn the_report_has_a_line_per_check_and_a_verdict() {
        let report = vec![
            ("dictionary".to_string(), Ok("219000 entries".to_string())),
            ("ocr".to_string(), Err("model missing".to_string())),
        ];

        let (text, passed) = format_report(&report);

        assert_eq!(
            text,
            "PASS dictionary: 219000 entries\nFAIL ocr: model missing\nSELF-TEST FAILED\n"
        );
        assert!(!passed);
        let (text, passed) = format_report(&[("pointer".to_string(), Ok("at (1, 2)".to_string()))]);
        assert_eq!(text, "PASS pointer: at (1, 2)\nSELF-TEST PASSED\n");
        assert!(passed);
    }

    #[test]
    fn the_resources_are_found_next_to_the_executable_or_in_a_resources_folder() {
        let directory = tempfile::tempdir().expect("dir");
        assert_eq!(find_resources(directory.path()), None);

        std::fs::create_dir_all(directory.path().join("resources").join("ocr")).expect("dirs");
        assert_eq!(
            find_resources(directory.path()),
            Some(directory.path().join("resources"))
        );

        std::fs::create_dir_all(directory.path().join("ocr")).expect("dirs");
        assert_eq!(
            find_resources(directory.path()),
            Some(directory.path().to_path_buf())
        );
    }

    #[test]
    fn a_missing_resources_folder_fails_every_file_check_with_a_reason() {
        let directory = tempfile::tempdir().expect("dir");

        let dictionary = check_dictionary(directory.path()).expect_err("no dictionary");
        let ocr = check_ocr(directory.path()).expect_err("no models");

        assert!(dictionary.contains("jmdict.db is missing"), "{dictionary}");
        assert!(!ocr.is_empty());
    }

    #[test]
    fn the_tokenizer_check_passes() {
        assert!(check_tokenizer().is_ok());
    }

    #[test]
    fn the_self_test_passes_on_the_bundled_files() {
        let resources = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources");
        if !resources.join("dictionary").join("jmdict.db").is_file()
            || !resources
                .join("ocr")
                .join(crate::ocr::meiki::RUNTIME_LIBRARY)
                .is_file()
        {
            // The bundled files are fetched by scripts and are not in git.
            return;
        }

        let dictionary = check_dictionary(&resources);
        let ocr = check_ocr(&resources);

        assert!(dictionary.is_ok(), "{dictionary:?}");
        assert!(ocr.is_ok(), "{ocr:?}");
    }
}
