// Generates the Flux Code icon set (PNG + ICO) from a programmatic drawing
// of the flux caret mark: a rounded dark square with a ">_" prompt glyph.
// Run: node scripts/gen-icons.mjs  (from the project root)
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const outDir = join(__dirname, "..", "src-tauri", "icons");
mkdirSync(outDir, { recursive: true });

const SIZE = 512;

// ---------- tiny CRC32 ----------
const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

// ---------- PNG encoder (RGBA, 8 bit) ----------
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([len, body, crc]);
}
function encodePng(width, height, rgba) {
  const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type RGBA
  const raw = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (width * 4 + 1)] = 0; // filter none
    rgba.copy(raw, y * (width * 4 + 1) + 1, y * width * 4, (y + 1) * width * 4);
  }
  const idat = deflateSync(raw, { level: 9 });
  return Buffer.concat([sig, chunk("IHDR", ihdr), chunk("IDAT", idat), chunk("IEND", Buffer.alloc(0))]);
}

// ---------- drawing helpers ----------
function distToSeg(px, py, ax, ay, bx, by) {
  const dx = bx - ax, dy = by - ay;
  const l2 = dx * dx + dy * dy;
  let t = l2 === 0 ? 0 : ((px - ax) * dx + (py - ay) * dy) / l2;
  t = Math.max(0, Math.min(1, t));
  const cx = ax + t * dx, cy = ay + t * dy;
  return Math.hypot(px - cx, py - cy);
}
function roundedRectCoverage(x, y, s, r) {
  // coverage test with 4-sample averaging for smooth corners
  let hit = 0;
  for (const [ox, oy] of [[0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]]) {
    const px = x + ox, py = y + oy;
    const cx = Math.max(r, Math.min(s - r, px));
    const cy = Math.max(r, Math.min(s - r, py));
    if (Math.hypot(px - cx, py - cy) <= r - 0.5) hit++;
  }
  return hit / 4;
}

const BG = [23, 23, 26];
const FG = [255, 255, 255];

function drawFlux(size) {
  const scale = size / SIZE;
  const rgba = Buffer.alloc(size * size * 4);
  // chevron ">" polyline points and cursor bar in 512-space
  const pts = [
    [168, 138],
    [302, 256],
    [168, 374],
  ];
  const strokeW = 58 * scale;
  const bar = { x: 330, y: 318, w: 26, h: 112 };
  const radius = 112 * scale;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      let r = 0, g = 0, b = 0, a = 0;
      const bgCov = roundedRectCoverage(x, y, size, radius);
      if (bgCov > 0) {
        r = BG[0]; g = BG[1]; b = BG[2]; a = bgCov * 255;
        // glyph coverage (3x supersample)
        let glyph = 0;
        for (let sy = 0; sy < 3; sy++) {
          for (let sx = 0; sx < 3; sx++) {
            const px = x + (sx + 0.5) / 3, py = y + (sy + 0.5) / 3;
            let inside = false;
            if (distToSeg(px, py, pts[0][0] * scale, pts[0][1] * scale, pts[1][0] * scale, pts[1][1] * scale) <= strokeW / 2) inside = true;
            if (distToSeg(px, py, pts[1][0] * scale, pts[1][1] * scale, pts[2][0] * scale, pts[2][1] * scale) <= strokeW / 2) inside = true;
            if (px >= bar.x * scale && px <= (bar.x + bar.w) * scale && py >= bar.y * scale && py <= (bar.y + bar.h) * scale) inside = true;
            if (inside) glyph++;
          }
        }
        const cov = glyph / 9;
        if (cov > 0) {
          r = r + (FG[0] - r) * cov;
          g = g + (FG[1] - g) * cov;
          b = b + (FG[2] - b) * cov;
        }
      }
      const i = (y * size + x) * 4;
      rgba[i] = Math.round(r);
      rgba[i + 1] = Math.round(g);
      rgba[i + 2] = Math.round(b);
      rgba[i + 3] = Math.round(a);
    }
  }
  return rgba;
}

function resize(rgba, from, to) {
  const out = Buffer.alloc(to * to * 4);
  const fx = from / to;
  for (let y = 0; y < to; y++) {
    for (let x = 0; x < to; x++) {
      const x0 = Math.floor(x * fx), x1 = Math.min(from, Math.ceil((x + 1) * fx));
      const y0 = Math.floor(y * fx), y1 = Math.min(from, Math.ceil((y + 1) * fx));
      let r = 0, g = 0, b = 0, a = 0, n = 0;
      for (let yy = y0; yy < y1; yy++) {
        for (let xx = x0; xx < x1; xx++) {
          const i = (yy * from + xx) * 4;
          r += rgba[i]; g += rgba[i + 1]; b += rgba[i + 2]; a += rgba[i + 3]; n++;
        }
      }
      const o = (y * to + x) * 4;
      out[o] = Math.round(r / n); out[o + 1] = Math.round(g / n);
      out[o + 2] = Math.round(b / n); out[o + 3] = Math.round(a / n);
    }
  }
  return out;
}

// ---------- ICO encoder (PNG-compressed entries) ----------
function encodeIco(sizes, pngBySize) {
  const entries = [];
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(sizes.length, 4);
  let offset = 6 + sizes.length * 16;
  for (const s of sizes) {
    const png = pngBySize.get(s);
    const e = Buffer.alloc(16);
    e[0] = s >= 256 ? 0 : s;
    e[1] = s >= 256 ? 0 : s;
    e[2] = 0; e[3] = 0;
    e.writeUInt16LE(1, 4); // planes
    e.writeUInt16LE(32, 6); // bpp
    e.writeUInt32LE(png.length, 8);
    e.writeUInt32LE(offset, 12);
    entries.push(e);
    offset += png.length;
  }
  return Buffer.concat([header, ...entries, ...sizes.map((s) => pngBySize.get(s))]);
}

// ---------- produce ----------
const big = drawFlux(SIZE);
const png512 = encodePng(SIZE, SIZE, big);
writeFileSync(join(outDir, "icon.png"), png512);

const pngBySize = new Map();
for (const s of [16, 24, 32, 48, 64, 128, 256]) {
  const small = resize(big, SIZE, s);
  pngBySize.set(s, encodePng(s, s, small));
}
writeFileSync(join(outDir, "32x32.png"), pngBySize.get(32));
writeFileSync(join(outDir, "128x128.png"), pngBySize.get(128));
writeFileSync(join(outDir, "128x128@2x.png"), pngBySize.get(256));
writeFileSync(join(outDir, "icon.ico"), encodeIco([16, 24, 32, 48, 64, 128, 256], pngBySize));

console.log("icons written to", outDir);
