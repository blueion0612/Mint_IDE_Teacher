use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::Emitter;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    student_id: String,
    timestamp: String,
    hash_check: String,
}

#[derive(Debug, Clone, Serialize)]
struct StudentEntry {
    student_id: String,
    folder_name: String,
    folder_path: String,
    timestamp: String,
    has_code_zip: bool,
    has_video_zip: bool,
    video_count: usize, // recordings in the new-format video/ dir
    status: String, // "pending", "success", "error"
    message: String,
}

/// True for the recording containers the exam IDE actually ships (mp4 from
/// ffmpeg, mov from the macOS screencapture fallback, plus legacy formats).
/// Both the counter and the decrypt loop must use this: Explorer/Finder drop
/// `Thumbs.db` / `desktop.ini` / `.DS_Store` sidecars into browsed folders,
/// and copying+XOR-ing those shipped garbled files and inflated the count.
fn is_video_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["mp4", "mov", "avi", "mkv", "webm"]
        .iter()
        .any(|ext| lower.rsplit_once('.').map(|(_, e)| e == *ext).unwrap_or(false))
}

/// Count real recordings in a submission's `video/` dir (new format), ignoring
/// macOS junk (`._*`, `.DS_Store`) and non-video sidecars. Lets the scan table
/// show whether a student actually has recordings — exactly where a proctor
/// would notice a missing one — instead of only reflecting the legacy
/// `submission_video.zip`.
fn count_video_dir(student_dir: &Path) -> usize {
    let vdir = student_dir.join("video");
    let mut n = 0;
    if let Ok(entries) = std::fs::read_dir(&vdir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() { continue; }
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            if name.starts_with("._") || !is_video_file(&name) { continue; }
            n += 1;
        }
    }
    n
}

#[derive(Debug, Clone, Serialize)]
struct ScanResult {
    students: Vec<StudentEntry>,
    total: usize,
}

#[derive(Debug, Clone, Serialize)]
struct DecryptProgress {
    index: usize,
    total: usize,
    student_id: String,
    status: String,
    message: String,
}

/// Same hash function as the exam IDE
fn hash_student_id(student_id: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"MINT_EXAM_2026_");
    hasher.update(student_id.as_bytes());
    hex::encode(hasher.finalize())
}

/// App version from tauri.conf.json — shown in the header so the grader can
/// see at a glance whether the latest build is installed.
#[tauri::command]
fn get_app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
fn scan_submissions(folder_path: String) -> Result<ScanResult, String> {
    let root = Path::new(&folder_path);
    if !root.is_dir() {
        return Err("Selected path is not a directory".to_string());
    }

    let mut students = Vec::new();

    let entries = std::fs::read_dir(root).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let folder_name = path.file_name().unwrap().to_string_lossy().to_string();

        // Check for manifest.json
        let manifest_path = path.join("manifest.json");
        if !manifest_path.exists() {
            // A folder WITHOUT a manifest but WITH submission artifacts is a
            // broken submission (crash mid-submit, manual copy) — surface it
            // as an error row instead of silently hiding the student from the
            // scan table. Unrelated folders (no artifacts) stay skipped.
            let has_code = path.join("submission_code.zip").exists();
            let has_video_zip = path.join("submission_video.zip").exists();
            let vcount = count_video_dir(&path);
            if has_code || has_video_zip || vcount > 0 {
                students.push(StudentEntry {
                    student_id: "unknown".to_string(),
                    folder_name,
                    folder_path: path.to_string_lossy().to_string(),
                    timestamp: String::new(),
                    has_code_zip: has_code,
                    has_video_zip,
                    video_count: vcount,
                    status: "error".to_string(),
                    message: "manifest.json missing (incomplete submission?)".to_string(),
                });
            }
            continue;
        }

        let manifest_str = std::fs::read_to_string(&manifest_path).unwrap_or_default();
        let manifest: Result<Manifest, _> = serde_json::from_str(&manifest_str);

        match manifest {
            Ok(m) => {
                // Verify hash_check
                let expected_hash = hash_student_id(&m.student_id);
                let valid = m.hash_check.len() == 16 && expected_hash.starts_with(&m.hash_check);

                students.push(StudentEntry {
                    student_id: m.student_id,
                    folder_name: folder_name.clone(),
                    folder_path: path.to_string_lossy().to_string(),
                    timestamp: m.timestamp,
                    has_code_zip: path.join("submission_code.zip").exists(),
                    has_video_zip: path.join("submission_video.zip").exists(),
                    video_count: count_video_dir(&path),
                    status: if valid { "pending".to_string() } else { "error".to_string() },
                    message: if valid {
                        "Ready to decrypt".to_string()
                    } else {
                        "Hash verification failed".to_string()
                    },
                });
            }
            Err(_) => {
                students.push(StudentEntry {
                    student_id: "unknown".to_string(),
                    folder_name,
                    folder_path: path.to_string_lossy().to_string(),
                    timestamp: String::new(),
                    has_code_zip: false,
                    has_video_zip: false,
                    video_count: 0,
                    status: "error".to_string(),
                    message: "Invalid manifest.json".to_string(),
                });
            }
        }
    }

    // Sort by student ID
    students.sort_by(|a, b| a.student_id.cmp(&b.student_id));
    let total = students.len();

    Ok(ScanResult { students, total })
}

#[tauri::command]
async fn decrypt_all(
    app_handle: tauri::AppHandle,
    folder_path: String,
    output_path: String,
) -> Result<String, String> {
    let out_root = PathBuf::from(&output_path);
    std::fs::create_dir_all(&out_root).map_err(|e| e.to_string())?;

    // Refuse an output folder that IS the submissions folder or is nested inside
    // it. decrypt writes <out>/<id>/{video/, manifest.json}, which is byte-for-
    // byte shaped like a new-format submission — so a second run would rescan
    // the decrypted OUTPUT as if it were submissions and self-copy each video
    // onto itself (truncating it to 0 bytes on some platforms, or re-obfuscating
    // the now-cleartext file), silently corrupting results while still reporting
    // success. Canonicalize both and reject the overlap up front.
    {
        let src_c = Path::new(&folder_path).canonicalize().map_err(|e| e.to_string())?;
        let out_c = out_root.canonicalize().map_err(|e| e.to_string())?;
        if out_c == src_c || out_c.starts_with(&src_c) {
            return Err("출력 폴더가 제출물 폴더와 같거나 그 하위에 있습니다. 제출물 폴더 바깥의 다른 폴더를 선택하세요.".to_string());
        }
    }

    let scan = scan_submissions(folder_path)?;

    let total = scan.students.len();
    let mut success_count = 0;
    // Student IDs already written THIS run. A student who submitted twice
    // (two MINT_Exam_* folders, same id) previously decrypted both into the
    // same out/<id>/ — the second silently overwrote the first's code. Route
    // repeats to out/<id>__<folder>/ so the grader sees both.
    let mut used_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (i, student) in scan.students.iter().enumerate() {
        if student.status == "error" {
            let _ = app_handle.emit("decrypt-progress", DecryptProgress {
                index: i,
                total,
                student_id: student.student_id.clone(),
                status: "skip".to_string(),
                message: student.message.clone(),
            });
            continue;
        }

        // Re-validate the student_id exactly like the exam IDE does before
        // it is interpolated into a folder name / SHA-256 password. The
        // manifest is student-craftable, so an id like `..\..\evil` or an
        // absolute path could otherwise escape out_root.
        let id = student.student_id.trim();
        if id.is_empty() || id.len() > 32 || id.chars().any(|c| !c.is_ascii_alphanumeric()) {
            let _ = app_handle.emit("decrypt-progress", DecryptProgress {
                index: i,
                total,
                student_id: student.student_id.clone(),
                status: "error".to_string(),
                message: "Invalid student_id".to_string(),
            });
            continue;
        }

        let password = hash_student_id(id);
        let student_dir = if used_ids.insert(id.to_string()) {
            out_root.join(id)
        } else {
            // Same id again this run — disambiguate by (sanitized) source
            // folder name instead of overwriting the earlier decrypt.
            let safe_folder: String = student.folder_name.chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
                .collect();
            out_root.join(format!("{}__{}", id, safe_folder))
        };
        // Defense-in-depth: ensure the (canonicalized) target stays inside out_root.
        if let (Ok(root_c), Some(parent)) = (out_root.canonicalize(), student_dir.parent()) {
            if let Ok(parent_c) = parent.canonicalize() {
                if !parent_c.starts_with(&root_c) {
                    let _ = app_handle.emit("decrypt-progress", DecryptProgress {
                        index: i,
                        total,
                        student_id: student.student_id.clone(),
                        status: "error".to_string(),
                        message: "Output path escapes target directory".to_string(),
                    });
                    continue;
                }
            }
        }
        let _ = std::fs::create_dir_all(&student_dir);

        let mut had_error = false;

        // Decrypt code zip
        let mut msg = String::new();
        let code_zip = PathBuf::from(&student.folder_path).join("submission_code.zip");
        if code_zip.exists() {
            let code_out = student_dir.join("code");
            match extract_encrypted_zip(&code_zip, &code_out, &password) {
                Ok(count) => msg.push_str(&format!("Code: {} files", count)),
                Err(e) => {
                    had_error = true;
                    msg.push_str(&format!("Code error: {}", e));
                }
            }
        }

        // Restore video files (deobfuscate headers OR decrypt zip)
        let video_dir_src = PathBuf::from(&student.folder_path).join("video");
        let video_zip = PathBuf::from(&student.folder_path).join("submission_video.zip");

        if video_dir_src.exists() {
            // New format: obfuscated video files (v2.0+)
            let video_out = student_dir.join("video");
            let _ = std::fs::create_dir_all(&video_out);
            let mut vcount = 0;
            if let Ok(entries) = std::fs::read_dir(&video_dir_src) {
                for entry in entries.flatten() {
                    let src = entry.path();
                    if !src.is_file() { continue; }
                    // Skip AppleDouble `._*` sidecars and anything that isn't a
                    // recording container (`.DS_Store`, `Thumbs.db`,
                    // `desktop.ini`, …) — XOR-"deobfuscating" those ships
                    // garbled files and inflates the "Video: N files" count.
                    let fname = src.file_name().unwrap_or_default().to_string_lossy().to_string();
                    if fname.starts_with("._") || !is_video_file(&fname) { continue; }
                    let dest = video_out.join(src.file_name().unwrap());
                    if let Err(e) = std::fs::copy(&src, &dest) {
                        had_error = true;
                        if !msg.is_empty() { msg.push_str(", "); }
                        msg.push_str(&format!("Video copy error: {}", e));
                        continue;
                    }
                    // Deobfuscate = same XOR operation reverses it
                    if let Err(e) = deobfuscate_video(&dest, password.as_bytes()) {
                        had_error = true;
                        if !msg.is_empty() { msg.push_str(", "); }
                        msg.push_str(&format!("Video error: {}", e));
                        continue;
                    }
                    vcount += 1;
                }
            }
            if !msg.is_empty() { msg.push_str(", "); }
            msg.push_str(&format!("Video: {} files", vcount));
        } else if video_zip.exists() {
            // Old format: encrypted zip (v1.x)
            let video_out = student_dir.join("video");
            match extract_encrypted_zip(&video_zip, &video_out, &password) {
                Ok(count) => {
                    if !msg.is_empty() { msg.push_str(", "); }
                    msg.push_str(&format!("Video: {} files", count));
                }
                Err(e) => {
                    had_error = true;
                    if !msg.is_empty() { msg.push_str(", "); }
                    msg.push_str(&format!("Video error: {}", e));
                }
            }
        }

        // Copy manifest
        let src_manifest = PathBuf::from(&student.folder_path).join("manifest.json");
        if src_manifest.exists() {
            let _ = std::fs::copy(&src_manifest, student_dir.join("manifest.json"));
        }

        if !had_error {
            success_count += 1;
        }

        let _ = app_handle.emit("decrypt-progress", DecryptProgress {
            index: i,
            total,
            student_id: student.student_id.clone(),
            status: if had_error { "error".to_string() } else { "success".to_string() },
            message: msg,
        });
    }

    let failed = total - success_count;
    Ok(format!("{} succeeded, {} failed, {} total (output: {})", success_count, failed, total, output_path))
}

fn extract_encrypted_zip(zip_path: &Path, output_dir: &Path, password: &str) -> Result<usize, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut count: usize = 0;
    let mut failed = 0usize;

    std::fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        // Per-entry error handling: a single corrupt member (truncated on a bad
        // USB copy) must NOT abort extraction of the rest — otherwise every
        // salvageable file after it is lost and the student is marked a total
        // failure. Record and continue instead.
        let mut entry = match archive.by_index_decrypt(i, password.as_bytes()) {
            Ok(e) => e,
            Err(_) => { failed += 1; continue; }
        };

        // Use enclosed_name() to guard against zip-slip: it returns a path
        // guaranteed to be relative and free of `..` / drive roots, or None
        // for an unsafe entry which we simply skip.
        let safe = match entry.enclosed_name() {
            Some(p) => p,
            None => continue,
        };
        let is_dir = entry.is_dir();

        let out_path = output_dir.join(&safe);

        if is_dir {
            let _ = std::fs::create_dir_all(&out_path);
        } else {
            if let Some(parent) = out_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            // Stream entry → file. The legacy v1.x submission_video.zip holds
            // multi-GB recordings; the old read_to_end buffered each whole
            // member in RAM before writing it out.
            let mut out = match std::fs::File::create(&out_path) {
                Ok(f) => f,
                Err(_) => { failed += 1; continue; }
            };
            match std::io::copy(&mut entry, &mut out) {
                Ok(_) => count += 1,
                Err(_) => {
                    failed += 1;
                    // Don't leave a half-written file that looks extracted.
                    drop(out);
                    let _ = std::fs::remove_file(&out_path);
                }
            }
        }
    }

    if failed > 0 {
        return Err(format!("{} files extracted, {} entries failed (corrupt/unreadable)", count, failed));
    }
    Ok(count)
}

/// Reverse XOR obfuscation on video file headers (same as obfuscate).
/// Only the first 1024 bytes are touched in place, so this never loads the
/// whole (multi-GB) video into RAM — mirrors the exam IDE's obfuscate_video.
fn deobfuscate_video(path: &Path, key: &[u8]) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::{Read, Seek, SeekFrom, Write};

    if key.is_empty() {
        return Err("empty obfuscation key".to_string());
    }

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("open {} failed: {}", path.display(), e))?;

    // Fill up to 1024 bytes (or EOF) — a single `read` may legally return short
    // on network/FUSE filesystems, which would leave a tail obfuscated and the
    // video unplayable. MUST stay symmetric with the IDE's obfuscate_video,
    // which fills the same way, so both touch the identical byte range.
    let mut buf = [0u8; 1024];
    let mut n = 0;
    while n < buf.len() {
        match file.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) => return Err(e.to_string()),
        }
    }
    for i in 0..n {
        buf[i] ^= key[i % key.len()];
    }
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_app_version,
            scan_submissions,
            decrypt_all,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MINT Grader");
}
