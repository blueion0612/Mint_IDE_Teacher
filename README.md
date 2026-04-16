# MINT Grader (Teacher)

Batch decrypt and review student exam submissions from MINT Exam IDE.

## Features

- Select folder containing student submissions
- Auto-detect student IDs from manifest files
- Hash verification (detect tampered submissions)
- One-click batch AES-256 decryption
- Extracts code, activity logs, and screen recordings per student

## Build

```bash
npm install
npx tauri build
```

## Prerequisites

- Node.js >= 18, Rust >= 1.70

## Install

Download the installer from [Releases](../../releases) and run it. Desktop shortcut is created automatically.

## Usage

1. Collect all student `MINT_Exam_*` folders into one directory
2. Open MINT Grader → Select that directory
3. Review detected students → Select output folder
4. Click "Decrypt All Submissions"
5. Results are organized by student ID with code, logs, and video
