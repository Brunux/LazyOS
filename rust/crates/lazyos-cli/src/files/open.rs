//! The one opener table (spec 009): extension → family → the program that
//! opens it, the glyph a row draws and whether a thumbnail can be made.
//!
//! It used to be three copies kept in step by hand - `rofi-files`' awk icon
//! table, its `case` for opening, and `lazyos-view` for a double click - and a
//! difference between them was a file that opened one way from the list and
//! another from the window. Now every surface asks [`family`] and [`plan`], and
//! `lazyos-view` is a link to this binary (`lazyos open`).
//!
//! The order of the decision is the old scripts' order, and each step is there
//! because the step after it gets the answer wrong:
//!
//! 1. **a document goes to `papers`** before the text test: a PDF's first page
//!    is often plain ASCII, and the NUL test would open it as garbage in `bat`;
//! 2. **a picture goes to `imv`** (chafa's sixels in foot when imv is missing)
//!    before the text test too: an SVG *is* text, and it has to be looked at;
//! 3. **text** - no NUL in the first 4 kB - goes to `bat` in the floated popup;
//! 4. **a video goes to `mpv`**, with `--no-config` and the mpris plugin by path;
//! 5. **everything else goes to the browser**: an HTML page that is not text,
//!    an EPUB, a `.deb`, and every branch whose program is missing.

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use lazyos_ui::Glyph;

/// What a file is, for the row, the thumbnail and the opener.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// A picture `lazyos-thumbnail` can decode (ffmpeg).
    Picture,
    /// An SVG: a picture for the opener, but no thumbnail (ffmpeg cannot
    /// rasterise it).
    Vector,
    /// An image format ffmpeg cannot thumbnail and imv may not read (HEIC, a
    /// camera raw, PSD): a picture glyph, opened by the text test's fallback.
    RawImage,
    Video,
    /// PDF: papers opens it and poppler draws its thumbnail.
    Pdf,
    /// The other formats papers reads, with no decoder for a thumbnail.
    Paper,
    Html,
    Office,
    Program,
    Archive,
    Audio,
    Text,
    /// An extension nobody listed: the generic program glyph, as before.
    Unknown,
}

/// One row of the table.
pub struct Rule {
    pub family: Family,
    pub exts: &'static [&'static str],
}

/// The table, in the order the old icon `awk` tested it. An extension appears
/// once; `opener_table_has_no_duplicate` holds it to that.
pub const RULES: &[Rule] = &[
    Rule {
        family: Family::Picture,
        exts: &[
            "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "avif",
        ],
    },
    Rule {
        family: Family::Video,
        exts: &[
            "mp4", "mkv", "webm", "mov", "avi", "m4v", "mpg", "mpeg", "ogv", "wmv", "flv", "3gp",
        ],
    },
    Rule {
        family: Family::Pdf,
        exts: &["pdf"],
    },
    Rule {
        family: Family::Paper,
        exts: &["djvu", "djv", "cbz", "cbr", "cb7", "cbt", "ai"],
    },
    Rule {
        family: Family::Html,
        exts: &["html", "htm", "xhtml", "shtml"],
    },
    Rule {
        family: Family::Office,
        exts: &[
            "odt", "ods", "odp", "odg", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "rtf", "epub",
        ],
    },
    Rule {
        family: Family::Program,
        exts: &[
            "sh", "bash", "zsh", "fish", "py", "pl", "rb", "lua", "awk", "deb", "appimage", "run",
            "bin", "exe", "elf", "o", "so", "a", "ko",
        ],
    },
    Rule {
        family: Family::Archive,
        exts: &[
            "tar", "gz", "tgz", "bz2", "tbz2", "xz", "txz", "zst", "zstd", "zip", "7z", "rar",
            "lz", "lz4", "lzma", "iso", "img",
        ],
    },
    Rule {
        family: Family::Audio,
        exts: &[
            "mp3", "flac", "ogg", "oga", "opus", "wav", "m4a", "aac", "wma", "aiff", "aif", "mid",
            "midi",
        ],
    },
    Rule {
        family: Family::Vector,
        exts: &["svg"],
    },
    Rule {
        family: Family::RawImage,
        exts: &[
            "svgz", "heic", "heif", "jxl", "raw", "cr2", "cr3", "nef", "arw", "dng", "psd", "xcf",
            "ico", "icns",
        ],
    },
    Rule {
        family: Family::Text,
        exts: &[
            // documents and logs, data and configuration, markup and scripts,
            // source code: one family, the old table's four lines.
            "txt", "text", "log", "out", "err", "nfo", "md", "markdown", "rst", "csv", "tsv",
            "json", "yml", "yaml", "toml", "ini", "cfg", "conf", "config", "env", "lock", "list",
            "xml", "rasi", "desktop", "service", "rules", "sql", "diff", "patch", "c", "h", "cc",
            "cpp", "hpp", "go", "rs", "java", "js", "mjs", "ts", "css", "scss", "less", "vim",
            "el", "tex", "bib",
        ],
    },
];

impl Family {
    /// The row's picture, from the TUI alphabet (a Nerd Font codepoint).
    pub fn glyph(self) -> Glyph {
        match self {
            Family::Picture | Family::Vector | Family::RawImage => Glyph::Image,
            Family::Video => Glyph::Video,
            Family::Pdf | Family::Paper => Glyph::Pdf,
            Family::Html => Glyph::Html,
            Family::Office => Glyph::Document,
            Family::Program | Family::Unknown => Glyph::Executable,
            Family::Archive => Glyph::Archive,
            Family::Audio => Glyph::Music,
            Family::Text => Glyph::File,
        }
    }

    /// The LazyOS theme name the rofi list drew for this family - what a card
    /// about the file asks mako for, and what `check-system.sh` holds the theme
    /// to. A thumbnail family names its fallback picture.
    pub fn icon(self) -> &'static str {
        match self {
            Family::Picture | Family::Vector | Family::RawImage => "image-x-generic",
            Family::Video => "video-x-generic",
            Family::Pdf | Family::Paper => "application-pdf",
            Family::Html => "text-html",
            Family::Office => "x-office-document",
            Family::Program | Family::Unknown => "application-x-executable",
            Family::Archive => "package-x-generic",
            Family::Audio => "audio-x-generic",
            Family::Text => "text-x-generic",
        }
    }

    /// Whether `lazyos-thumbnail` can turn this into a picture: ffmpeg for an
    /// image or a video frame, poppler for a PDF's first page.
    pub fn thumbnail(self) -> bool {
        matches!(self, Family::Picture | Family::Video | Family::Pdf)
    }

    pub fn label(self) -> &'static str {
        match self {
            Family::Picture | Family::Vector | Family::RawImage => "picture",
            Family::Video => "video",
            Family::Pdf | Family::Paper => "document",
            Family::Html => "web page",
            Family::Office => "office document",
            Family::Program => "program",
            Family::Archive => "archive",
            Family::Audio => "audio",
            Family::Text => "text",
            Family::Unknown => "file",
        }
    }
}

/// The extension of a file name, lowercased: none for a dotfile (`.bashrc`) or
/// a name without a dot - the old awk's `ext()`.
pub fn extension(name: &str) -> Option<String> {
    let name = name.rsplit('/').next().unwrap_or(name);
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// The family of a file, from its name and whether it is executable - the two
/// facts a listing already has, so a row never costs a `file` call.
pub fn family(name: &str, executable: bool) -> Family {
    match extension(name) {
        Some(ext) => RULES
            .iter()
            .find(|rule| rule.exts.contains(&ext.as_str()))
            .map_or(Family::Unknown, |rule| rule.family),
        // A program with no extension is a program; anything else is usually
        // a README.
        None if executable => Family::Program,
        None => Family::Text,
    }
}

/// What a file is opened with, as the argv to start: the decision without the
/// side effect, so the tests can read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub argv: Vec<String>,
    /// Say imv's keys once per session first (`lazyos-image-hint`).
    pub image_hint: bool,
}

/// Why a path cannot be opened at all - what the card says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    Missing,
    Unreadable,
    NoProgram,
}

impl Refusal {
    pub fn message(&self) -> &'static str {
        match self {
            Refusal::Missing => "Broken link, there is nothing there",
            Refusal::Unreadable => "Not readable by this user",
            Refusal::NoProgram => "No program is installed to open this file",
        }
    }
}

/// What the opener can find on this machine: the programs on `PATH` and a few
/// files. A trait so a test answers instead of the machine.
pub trait Tools {
    fn has(&self, program: &str) -> bool;
    fn readable(&self, path: &Path) -> bool;
    /// This binary, for a directory (`lazyos menu files DIR`).
    fn exe(&self) -> String;
}

/// The session's tools: `PATH` and the filesystem.
pub struct System {
    path: String,
    exe: String,
}

impl System {
    pub fn from_env() -> Self {
        Self {
            path: std::env::var("PATH").unwrap_or_default(),
            exe: std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "lazyos".into()),
        }
    }
}

impl Tools for System {
    fn has(&self, program: &str) -> bool {
        on_path(&self.path, program)
    }

    fn readable(&self, path: &Path) -> bool {
        Path::new(path).is_file()
    }

    fn exe(&self) -> String {
        self.exe.clone()
    }
}

/// `command -v`, without a shell.
pub fn on_path(path: &str, program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.split(':').filter(|d| !d.is_empty()).any(|dir| {
        std::fs::metadata(Path::new(dir).join(program))
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    })
}

/// The first 4 kB carry no NUL: what both scripts called text (`bat` would
/// draw a binary as an empty page). An empty file is text.
pub fn is_text(path: &Path) -> bool {
    let mut head = [0u8; 4096];
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut filled = 0;
    while filled < head.len() {
        match file.read(&mut head[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return false,
        }
    }
    !head[..filled].contains(&0)
}

const MPRIS: &str = "/etc/mpv/scripts/mpris.so";

/// Decide how `path` opens. `text` is the NUL test's answer for it (passed in,
/// so the decision stays pure).
pub fn plan(path: &Path, text: bool, tools: &dyn Tools) -> Result<Plan, Refusal> {
    let shown = path.display().to_string();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| shown.clone());
    let meta = std::fs::metadata(path);
    let Ok(meta) = meta else {
        return Err(Refusal::Missing);
    };
    let run = |argv: &[&str]| Plan {
        argv: argv.iter().map(|s| s.to_string()).collect(),
        image_hint: false,
    };
    if meta.is_dir() {
        let exe = tools.exe();
        return Ok(run(&[&exe, "menu", "files", "--", &shown]));
    }
    if !tools.readable(path) || File::open(path).is_err() {
        return Err(Refusal::Unreadable);
    }
    let browser = || -> Result<Plan, Refusal> {
        if tools.has("firefox-esr") {
            let url = format!("file://{shown}");
            Ok(run(&["firefox-esr", &url]))
        } else {
            Err(Refusal::NoProgram)
        }
    };
    use std::os::unix::fs::PermissionsExt;
    let family = family(&name, meta.permissions().mode() & 0o111 != 0);
    match family {
        Family::Pdf | Family::Paper => {
            return if tools.has("papers") {
                Ok(run(&["papers", &shown]))
            } else {
                browser()
            };
        }
        Family::Picture | Family::Vector => {
            if tools.has("imv") {
                let mut plan = run(&["imv", "--", &shown]);
                plan.image_hint = true;
                return Ok(plan);
            }
            if tools.has("chafa") && tools.has("foot") {
                // --hold keeps the window after chafa has drawn once; the sleep
                // waits for foot to resize the pty, so chafa measures the real
                // window and not an 80x24 corner.
                let title = format!("--title={name}");
                return Ok(run(&[
                    "foot",
                    "--hold",
                    "--app-id=popup",
                    &title,
                    "-e",
                    "sh",
                    "-c",
                    "sleep 0.2; exec chafa -f sixels \"$1\"",
                    "chafa",
                    &shown,
                ]));
            }
        }
        _ => {}
    }
    if text && tools.has("bat") && tools.has("foot") {
        let title = format!("--title={name}");
        let file_name = format!("--file-name={name}");
        // BAT_THEME=ansi is what the shell profile sets; foot runs the command
        // directly, so it is passed here or bat paints its own theme.
        return Ok(run(&[
            "foot",
            "--app-id=popup",
            &title,
            "env",
            "BAT_THEME=ansi",
            "bat",
            "--paging=always",
            "--style=numbers",
            &file_name,
            "--",
            &shown,
        ]));
    }
    if family == Family::Video && tools.has("mpv") {
        let title = format!("--title={name}");
        let mut argv = vec![
            "mpv",
            "--no-config",
            "--no-resume-playback",
            "--force-window=yes",
            "--keep-open=no",
            &title,
        ];
        // --no-config also empties the script directories mpv-mpris lives in,
        // so playerctl and the Media menu only see the film with it by path.
        let script = format!("--script={MPRIS}");
        if tools.readable(Path::new(MPRIS)) {
            argv.push(&script);
        }
        argv.push("--");
        argv.push(&shown);
        return Ok(run(&argv));
    }
    browser()
}

/// A path from a desktop handler: GIO passes a `file://` URI for `%U`, and
/// anything percent-encoded in it is decoded; a relative path is made
/// absolute against the current directory.
pub fn path_from_arg(arg: &str, cwd: &Path) -> Option<PathBuf> {
    let raw = match arg.strip_prefix("file://") {
        Some(rest) => {
            // file://host/path: only the local host is meaningful here.
            let rest = rest.strip_prefix("localhost").unwrap_or(rest);
            percent_decode(rest)?
        }
        None => arg.to_string(),
    };
    if raw.is_empty() {
        return None;
    }
    let path = PathBuf::from(raw);
    Some(if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    })
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    // A path is bytes on Linux; a non-UTF-8 name is still a name.
    use std::os::unix::ffi::OsStringExt;
    Some(
        std::ffi::OsString::from_vec(out)
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    struct Fake {
        programs: Vec<&'static str>,
    }

    impl Tools for Fake {
        fn has(&self, program: &str) -> bool {
            self.programs.contains(&program)
        }
        fn readable(&self, path: &Path) -> bool {
            path != Path::new(MPRIS) && path.is_file()
        }
        fn exe(&self) -> String {
            "/usr/bin/lazyos".into()
        }
    }

    fn every() -> Fake {
        Fake {
            programs: vec![
                "papers",
                "imv",
                "chafa",
                "foot",
                "bat",
                "mpv",
                "firefox-esr",
            ],
        }
    }

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lazyos-open-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn opener_table_has_no_duplicate() {
        let mut seen = BTreeSet::new();
        for rule in RULES {
            for ext in rule.exts {
                assert!(seen.insert(*ext), "{ext} is in the table twice");
                assert_eq!(*ext, ext.to_ascii_lowercase(), "{ext} must be lowercase");
            }
        }
    }

    #[test]
    fn opener_table_is_total() {
        // Every name has a family and every family a glyph, a theme icon and
        // an answer to "can it be a thumbnail" - there is no name the list
        // draws without a picture.
        for name in [
            "a.jpg", "a.MKV", "a.pdf", "a.djvu", "a.html", "a.docx", "a.sh", "a.tar", "a.flac",
            "a.svg", "a.heic", "a.rs", "README", "a.weird", ".bashrc", "noext",
        ] {
            let family = family(name, false);
            assert!(!family.icon().is_empty());
            let _ = family.glyph().cell();
        }
        assert_eq!(family("a.weird", false), Family::Unknown);
        assert_eq!(family("README", false), Family::Text);
        assert_eq!(family("configure", true), Family::Program);
        assert_eq!(family(".bashrc", false), Family::Text);
        assert_eq!(family("Photo.JPG", false), Family::Picture);
        assert_eq!(family("film.tar.gz", false), Family::Archive);
    }

    #[test]
    fn the_thumbnail_families_are_what_lazyos_thumbnail_decodes() {
        let thumbs: Vec<&str> = RULES
            .iter()
            .filter(|rule| rule.family.thumbnail())
            .flat_map(|rule| rule.exts.iter().copied())
            .collect();
        for ext in ["jpg", "png", "webp", "mp4", "mkv", "pdf"] {
            assert!(thumbs.contains(&ext), "{ext}");
        }
        for ext in ["svg", "heic", "djvu", "txt"] {
            assert!(!thumbs.contains(&ext), "{ext} has no decoder");
        }
    }

    #[test]
    fn a_picture_goes_to_imv_with_the_hint_then_chafa_then_the_browser() {
        let dir = tree("picture");
        let path = dir.join("photo.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\n\0\0").unwrap();
        let plan = plan(&path, false, &every()).unwrap();
        assert_eq!(plan.argv[..2], ["imv", "--"]);
        assert!(plan.image_hint);
        let no_imv = Fake {
            programs: vec!["chafa", "foot", "firefox-esr"],
        };
        let chafa = super::plan(&path, false, &no_imv).unwrap();
        assert_eq!(chafa.argv[0], "foot");
        assert!(chafa.argv.iter().any(|a| a.contains("chafa -f sixels")));
        let bare = Fake {
            programs: vec!["firefox-esr"],
        };
        assert_eq!(
            super::plan(&path, false, &bare).unwrap().argv[0],
            "firefox-esr"
        );
    }

    #[test]
    fn an_svg_is_a_picture_even_though_it_is_text() {
        let dir = tree("svg");
        let path = dir.join("mark.svg");
        std::fs::write(&path, "<svg/>").unwrap();
        assert_eq!(plan(&path, true, &every()).unwrap().argv[0], "imv");
    }

    #[test]
    fn a_pdf_goes_to_papers_before_the_text_test() {
        let dir = tree("pdf");
        let path = dir.join("paper.pdf");
        std::fs::write(&path, "%PDF-1.4 plain ascii").unwrap();
        assert_eq!(plan(&path, true, &every()).unwrap().argv[0], "papers");
    }

    #[test]
    fn text_goes_to_bat_in_the_popup() {
        let dir = tree("text");
        let path = dir.join("notes.md");
        std::fs::write(&path, "# notes\n").unwrap();
        assert!(is_text(&path));
        let plan = plan(&path, true, &every()).unwrap();
        assert_eq!(plan.argv[..2], ["foot", "--app-id=popup"]);
        assert!(plan.argv.contains(&"BAT_THEME=ansi".to_string()));
        assert_eq!(plan.argv.last().unwrap(), &path.display().to_string());
    }

    #[test]
    fn a_video_goes_to_mpv_and_a_binary_to_the_browser() {
        let dir = tree("video");
        let video = dir.join("film.mkv");
        std::fs::write(&video, b"\x1a\x45\xdf\xa3\0").unwrap();
        assert!(!is_text(&video));
        let plan = plan(&video, false, &every()).unwrap();
        assert_eq!(plan.argv[0], "mpv");
        assert!(plan.argv.contains(&"--no-config".to_string()));
        let blob = dir.join("thing.deb");
        std::fs::write(&blob, b"!<arch>\n\0").unwrap();
        assert_eq!(
            super::plan(&blob, false, &every()).unwrap().argv[0],
            "firefox-esr"
        );
    }

    #[test]
    fn a_directory_opens_the_file_list_and_a_broken_link_is_refused() {
        let dir = tree("dir");
        let plan = plan(&dir, false, &every()).unwrap();
        assert_eq!(plan.argv[1..4], ["menu", "files", "--"]);
        let link = dir.join("gone");
        std::os::unix::fs::symlink(dir.join("nowhere"), &link).unwrap();
        assert_eq!(super::plan(&link, false, &every()), Err(Refusal::Missing));
        let blob = dir.join("x.bin");
        std::fs::write(&blob, b"\0").unwrap();
        let nothing = Fake { programs: vec![] };
        assert_eq!(super::plan(&blob, false, &nothing), Err(Refusal::NoProgram));
    }

    #[test]
    fn a_uri_from_gio_is_a_path() {
        let cwd = Path::new("/home/me");
        assert_eq!(
            path_from_arg("file:///home/me/My%20Photo.jpg", cwd),
            Some(PathBuf::from("/home/me/My Photo.jpg"))
        );
        assert_eq!(
            path_from_arg("file://localhost/tmp/a%25b", cwd),
            Some(PathBuf::from("/tmp/a%b"))
        );
        assert_eq!(
            path_from_arg("notes.md", cwd),
            Some(PathBuf::from("/home/me/notes.md"))
        );
        assert_eq!(path_from_arg("file:///bad%zz", cwd), None);
        assert_eq!(path_from_arg("", cwd), None);
    }
}
