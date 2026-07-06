// Reclaimer frontend logic.
// Wired to real Tauri backend commands: scan_image and open_file.

const { invoke } = window.__TAURI__.core;
const { open } = window.__TAURI__.dialog;
const { listen } = window.__TAURI__.event;

let imagePath = null;
let foundFiles = [];
let selectedIndex = null;

const openBtn = document.getElementById("open-btn");
const scanBtn = document.getElementById("scan-btn");
const imagePathDisplay = document.getElementById("image-path-display");
const resultsList = document.getElementById("results-list");
const detailPanel = document.getElementById("detail-panel");
const statusbar = document.getElementById("statusbar");
const progressTrack = document.getElementById("progress-track");
const progressFill = document.getElementById("progress-fill");

const statSectors = document.getElementById("stat-sectors");
const statFound = document.getElementById("stat-found");
const statRecovered = document.getElementById("stat-recovered");
const statSize = document.getElementById("stat-size");

listen("scan-progress", (event) => {
  const { scanned_sectors, total_sectors, found_count } = event.payload;

  statSectors.textContent = scanned_sectors;
  statFound.textContent = found_count;

  if (total_sectors > 0) {
    const pct = Math.min(
      100,
      Math.round((scanned_sectors / total_sectors) * 100),
    );
    progressFill.style.width = pct + "%";
  }
});

openBtn.addEventListener("click", async () => {
  console.log("[ui] open clicked");

  try {
    const selected = await open({
      multiple: false,
      filters: [
        { name: "Disk images", extensions: ["img", "dd", "raw"] },
        { name: "All files", extensions: ["*"] },
      ],
    });

    if (!selected) {
      console.log("[ui] file picker cancelled");
      return;
    }

    imagePath = selected;
    imagePathDisplay.textContent = imagePath;
    scanBtn.disabled = false;
    statusbar.textContent = "Image selected. Click Scan to begin.";
  } catch (e) {
    console.error("[ui] file picker failed:", e);
    statusbar.textContent = `Failed to open file picker: ${e}`;
  }
});

scanBtn.addEventListener("click", async () => {
  if (!imagePath) return;

  console.log("[ui] scan started on", imagePath);
  statusbar.textContent = "Scanning...";
  progressTrack.style.display = "block";
  progressFill.style.width = "0%";

  foundFiles = [];
  selectedIndex = null;
  statSectors.textContent = "0";
  statFound.textContent = "0";
  statRecovered.textContent = "0";
  statSize.textContent = "0 KB";
  renderResults();
  renderDetail();

  scanBtn.disabled = true;

  try {
    const results = await invoke("scan_image", { imagePath });

    foundFiles = results.map((f) => ({
      type: f.file_type,
      extension: f.extension,
      sector: f.sector,
      offset: f.offset,
      size: f.size_bytes,
      filename: f.filename,
    }));

    statFound.textContent = foundFiles.length;
    statRecovered.textContent = foundFiles.length;
    const totalBytes = foundFiles.reduce((sum, f) => sum + f.size, 0);
    statSize.textContent = Math.round(totalBytes / 1024) + " KB";

    statusbar.textContent = `Scan complete. ${foundFiles.length} files found.`;
  } catch (e) {
    console.error("[ui] scan failed:", e);
    statusbar.textContent = `Scan failed: ${e}`;
  } finally {
    progressTrack.style.display = "none";
    scanBtn.disabled = false;
    renderResults();
  }
});

function renderResults() {
  if (foundFiles.length === 0) {
    resultsList.innerHTML = `<div class="empty-state">No files found yet.</div>`;
    return;
  }

  resultsList.innerHTML = foundFiles
    .map((file, i) => {
      const selected = i === selectedIndex ? "selected" : "";
      const ext = file.extension.toLowerCase();
      return `
        <div class="row ${selected}" data-index="${i}">
          <div class="thumb">${file.type.slice(0, 3)}</div>
          <div class="row-info">
            <div class="row-name">${file.filename}</div>
            <div class="row-meta">sector ${file.sector} · offset ${file.offset}</div>
          </div>
          <span class="badge">${file.type}</span>
        </div>
      `;
    })
    .join("");

  resultsList.querySelectorAll(".row").forEach((row) => {
    row.addEventListener("click", () => {
      selectedIndex = parseInt(row.dataset.index, 10);
      renderResults();
      renderDetail();
    });
  });
}

function renderDetail() {
  if (selectedIndex === null || !foundFiles[selectedIndex]) {
    detailPanel.innerHTML = `
      <h3>Details</h3>
      <div class="empty-state">Select a file to see details.</div>
    `;
    return;
  }

  const file = foundFiles[selectedIndex];

  detailPanel.innerHTML = `
    <h3>Details</h3>
    <div class="detail-row"><span>Type</span><span>${file.type}</span></div>
    <div class="detail-row"><span>Sector</span><span>${file.sector}</span></div>
    <div class="detail-row"><span>Offset</span><span>${file.offset} B</span></div>
    <div class="detail-row"><span>Size</span><span>${(file.size / 1024).toFixed(1)} KB</span></div>
    <div class="detail-row"><span>Status</span><span style="color:#4ADE80">Recovered</span></div>
    <button class="recover-btn" id="recover-btn">Open file</button>
  `;

  document.getElementById("recover-btn").addEventListener("click", async () => {
    console.log("[ui] opening file:", file.filename);
    try {
      await invoke("open_file", { filename: file.filename });
    } catch (e) {
      console.error("[ui] failed to open file:", e);
      statusbar.textContent = `Failed to open file: ${e}`;
    }
  });
}
