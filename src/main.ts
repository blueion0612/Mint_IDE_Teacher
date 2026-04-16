import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

interface StudentEntry {
  student_id: string;
  folder_name: string;
  folder_path: string;
  timestamp: string;
  has_code_zip: boolean;
  has_video_zip: boolean;
  status: string;
  message: string;
}

interface ScanResult {
  students: StudentEntry[];
  total: number;
}

interface DecryptProgress {
  index: number;
  total: number;
  student_id: string;
  status: string;
  message: string;
}

let selectedFolder = "";
let outputFolder = "";
let students: StudentEntry[] = [];

document.addEventListener("DOMContentLoaded", () => {
  document.getElementById("btn-select-folder")!.addEventListener("click", selectFolder);
  document.getElementById("btn-select-output")!.addEventListener("click", selectOutputFolder);
  document.getElementById("btn-decrypt")!.addEventListener("click", decryptAll);

  listen<DecryptProgress>("decrypt-progress", (event) => {
    onProgress(event.payload);
  });
});

async function selectFolder(): Promise<void> {
  const path = await open({ directory: true, title: "Select submissions folder" });
  if (!path) return;

  selectedFolder = typeof path === "string" ? path : String(path);
  document.getElementById("selected-path")!.textContent = selectedFolder;

  // Scan
  try {
    const result = await invoke<ScanResult>("scan_submissions", { folderPath: selectedFolder });
    students = result.students;
    renderStudentList();
    show("step-scan");
    show("step-decrypt");
  } catch (e) {
    alert(`Scan failed: ${e}`);
  }
}

async function selectOutputFolder(): Promise<void> {
  const path = await open({ directory: true, title: "Select output folder" });
  if (!path) return;

  outputFolder = typeof path === "string" ? path : String(path);
  document.getElementById("output-path")!.textContent = outputFolder;
  (document.getElementById("btn-decrypt") as HTMLButtonElement).disabled = false;
}

async function decryptAll(): Promise<void> {
  if (!selectedFolder || !outputFolder) return;

  const btn = document.getElementById("btn-decrypt") as HTMLButtonElement;
  btn.disabled = true;
  btn.textContent = "Decrypting...";

  show("step-progress");
  document.getElementById("progress-log")!.innerHTML = "";

  try {
    const result = await invoke<string>("decrypt_all", {
      folderPath: selectedFolder,
      outputPath: outputFolder,
    });

    addLogLine(result, "success");
    btn.textContent = "Done!";

    // Open the output folder
    alert(`Complete!\n\n${result}\n\nOutput: ${outputFolder}`);
  } catch (e) {
    addLogLine(`Error: ${e}`, "error");
    btn.disabled = false;
    btn.textContent = "Retry";
  }
}

function onProgress(p: DecryptProgress): void {
  const pct = Math.round(((p.index + 1) / p.total) * 100);
  document.getElementById("progress-bar")!.style.width = `${pct}%`;
  document.getElementById("progress-text")!.textContent = `${p.index + 1} / ${p.total}`;

  const icon = p.status === "success" ? "OK" : p.status === "skip" ? "SKIP" : "ERR";
  addLogLine(`[${icon}] ${p.student_id} - ${p.message}`, p.status);

  // Update student list status
  const row = document.querySelector(`.student-row[data-id="${p.student_id}"] .student-status`);
  if (row) {
    row.className = `student-status ${p.status}`;
    row.textContent = p.status.toUpperCase();
  }
}

function renderStudentList(): void {
  const container = document.getElementById("student-list")!;
  container.innerHTML = "";

  for (const s of students) {
    const row = document.createElement("div");
    row.className = "student-row";
    row.dataset.id = s.student_id;

    const files: string[] = [];
    if (s.has_code_zip) files.push("code");
    if (s.has_video_zip) files.push("video");

    row.innerHTML = `
      <span class="student-id">${esc(s.student_id)}</span>
      <span class="student-time">${esc(s.timestamp)}</span>
      <span class="student-files">${files.join(" + ") || "no files"}</span>
      <span class="student-status ${s.status}">${s.status.toUpperCase()}</span>
    `;
    container.appendChild(row);
  }

  const pending = students.filter((s) => s.status === "pending").length;
  const errors = students.filter((s) => s.status === "error").length;
  document.getElementById("scan-summary")!.textContent =
    `Total: ${students.length} | Ready: ${pending} | Errors: ${errors}`;
}

function addLogLine(text: string, cls: string): void {
  const log = document.getElementById("progress-log")!;
  const line = document.createElement("div");
  line.className = `log-line ${cls}`;
  line.textContent = text;
  log.appendChild(line);
  log.scrollTop = log.scrollHeight;
}

function show(id: string): void {
  document.getElementById(id)!.classList.add("active");
}

function esc(text: string): string {
  const d = document.createElement("div");
  d.textContent = text;
  return d.innerHTML;
}
