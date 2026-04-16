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
    status: String, // "pending", "success", "error"
    message: String,
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
            continue;
        }

        let manifest_str = std::fs::read_to_string(&manifest_path).unwrap_or_default();
        let manifest: Result<Manifest, _> = serde_json::from_str(&manifest_str);

        match manifest {
            Ok(m) => {
                // Verify hash_check
                let expected_hash = hash_student_id(&m.student_id);
                let valid = expected_hash.starts_with(&m.hash_check);

                students.push(StudentEntry {
                    student_id: m.student_id,
                    folder_name: folder_name.clone(),
                    folder_path: path.to_string_lossy().to_string(),
                    timestamp: m.timestamp,
                    has_code_zip: path.join("submission_code.zip").exists(),
                    has_video_zip: path.join("submission_video.zip").exists(),
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
    let scan = scan_submissions(folder_path)?;
    let out_root = PathBuf::from(&output_path);
    std::fs::create_dir_all(&out_root).map_err(|e| e.to_string())?;

    let total = scan.students.len();
    let mut success_count = 0;

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

        let password = hash_student_id(&student.student_id);
        let student_dir = out_root.join(&student.student_id);
        let _ = std::fs::create_dir_all(&student_dir);

        // Decrypt code zip
        let mut msg = String::new();
        let code_zip = PathBuf::from(&student.folder_path).join("submission_code.zip");
        if code_zip.exists() {
            let code_out = student_dir.join("code");
            match extract_encrypted_zip(&code_zip, &code_out, &password) {
                Ok(count) => msg.push_str(&format!("Code: {} files", count)),
                Err(e) => msg.push_str(&format!("Code error: {}", e)),
            }
        }

        // Decrypt video zip
        let video_zip = PathBuf::from(&student.folder_path).join("submission_video.zip");
        if video_zip.exists() {
            let video_out = student_dir.join("video");
            match extract_encrypted_zip(&video_zip, &video_out, &password) {
                Ok(count) => {
                    if !msg.is_empty() { msg.push_str(", "); }
                    msg.push_str(&format!("Video: {} files", count));
                }
                Err(e) => {
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

        success_count += 1;

        let _ = app_handle.emit("decrypt-progress", DecryptProgress {
            index: i,
            total,
            student_id: student.student_id.clone(),
            status: "success".to_string(),
            message: msg,
        });
    }

    Ok(format!("{}/{} submissions decrypted to {}", success_count, total, output_path))
}

fn extract_encrypted_zip(zip_path: &Path, output_dir: &Path, password: &str) -> Result<usize, String> {
    use std::io::Read;

    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut count: usize = 0;

    std::fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        // by_index_decrypt returns Result<ZipResult<ZipFile>>
        // ZipResult is just Result<ZipFile, InvalidPassword> in some versions
        // Use the simpler approach: try with password via by_index
        let mut entry = archive
            .by_index_decrypt(i, password.as_bytes())
            .map_err(|e| format!("Error on entry {}: {}", i, e))?;

        // entry might be a nested Result — handle both shapes
        let name: String;
        let is_dir: bool;

        // The zip crate 2.x by_index_decrypt returns ZipResult which we
        // already unwrapped above with ?. entry is now a ZipFile.
        name = entry.name().replace('\\', "/");
        is_dir = entry.is_dir();

        let out_path = output_dir.join(&name);

        if is_dir {
            let _ = std::fs::create_dir_all(&out_path);
        } else {
            if let Some(parent) = out_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)
                .map_err(|e| format!("Read error: {}", e))?;
            std::fs::write(&out_path, &buf)
                .map_err(|e| format!("Write error: {}", e))?;
            count += 1;
        }
    }

    Ok(count)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            scan_submissions,
            decrypt_all,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MINT Grader");
}
