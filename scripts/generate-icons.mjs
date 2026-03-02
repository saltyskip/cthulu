#!/usr/bin/env node
/**
 * Generate Tauri app icons from the shared cthulu-logo.png.
 *
 * Creates: white logo on dark (#0b1317) background at all required sizes.
 * Outputs to packages/brand/assets/ (master icon) and cthulu-studio/src-tauri/icons/.
 *
 * Usage: node scripts/generate-icons.mjs
 */

import sharp from "sharp";
import { mkdirSync, writeFileSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { execSync } from "child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");

const LOGO_SRC = join(ROOT, "cthulu-site/public/cthulu-logo.png");
const BRAND_ASSETS = join(ROOT, "packages/brand/assets");
const TAURI_ICONS = join(ROOT, "cthulu-studio/src-tauri/icons");

const BG_COLOR = { r: 11, g: 19, b: 23, alpha: 255 }; // #0b1317 — eldritchDark.bg

// Tauri required icon sizes
const TAURI_SIZES = [
  { name: "32x32.png", size: 32 },
  { name: "64x64.png", size: 64 },
  { name: "128x128.png", size: 128 },
  { name: "128x128@2x.png", size: 256 },
  { name: "icon.png", size: 512 },
];

// Windows icon sizes (for StoreLogo and Square* icons)
const WINDOWS_SIZES = [
  { name: "Square30x30Logo.png", size: 30 },
  { name: "Square44x44Logo.png", size: 44 },
  { name: "Square71x71Logo.png", size: 71 },
  { name: "Square89x89Logo.png", size: 89 },
  { name: "Square107x107Logo.png", size: 107 },
  { name: "Square142x142Logo.png", size: 142 },
  { name: "Square150x150Logo.png", size: 150 },
  { name: "Square284x284Logo.png", size: 284 },
  { name: "Square310x310Logo.png", size: 310 },
  { name: "StoreLogo.png", size: 50 },
];

// iOS icon sizes
const IOS_SIZES = [
  { name: "ios/AppIcon-20x20@1x.png", size: 20 },
  { name: "ios/AppIcon-20x20@2x.png", size: 40 },
  { name: "ios/AppIcon-20x20@3x.png", size: 60 },
  { name: "ios/AppIcon-29x29@1x.png", size: 29 },
  { name: "ios/AppIcon-29x29@2x.png", size: 58 },
  { name: "ios/AppIcon-29x29@3x.png", size: 87 },
  { name: "ios/AppIcon-40x40@1x.png", size: 40 },
  { name: "ios/AppIcon-40x40@2x.png", size: 80 },
  { name: "ios/AppIcon-40x40@3x.png", size: 120 },
  { name: "ios/AppIcon-60x60@2x.png", size: 120 },
  { name: "ios/AppIcon-60x60@3x.png", size: 180 },
  { name: "ios/AppIcon-76x76@1x.png", size: 76 },
  { name: "ios/AppIcon-76x76@2x.png", size: 152 },
  { name: "ios/AppIcon-83.5x83.5@2x.png", size: 167 },
  { name: "ios/AppIcon-1024x1024@1x.png", size: 1024 },
];

// Android icon sizes
const ANDROID_SIZES = [
  { name: "android/mipmap-mdpi/ic_launcher.png", size: 48 },
  { name: "android/mipmap-hdpi/ic_launcher.png", size: 72 },
  { name: "android/mipmap-xhdpi/ic_launcher.png", size: 96 },
  { name: "android/mipmap-xxhdpi/ic_launcher.png", size: 144 },
  { name: "android/mipmap-xxxhdpi/ic_launcher.png", size: 192 },
  { name: "android/mipmap-mdpi/ic_launcher_round.png", size: 48 },
  { name: "android/mipmap-hdpi/ic_launcher_round.png", size: 72 },
  { name: "android/mipmap-xhdpi/ic_launcher_round.png", size: 96 },
  { name: "android/mipmap-xxhdpi/ic_launcher_round.png", size: 144 },
  { name: "android/mipmap-xxxhdpi/ic_launcher_round.png", size: 192 },
  { name: "android/mipmap-mdpi/ic_launcher_foreground.png", size: 48 },
  { name: "android/mipmap-hdpi/ic_launcher_foreground.png", size: 72 },
  { name: "android/mipmap-xhdpi/ic_launcher_foreground.png", size: 96 },
  { name: "android/mipmap-xxhdpi/ic_launcher_foreground.png", size: 144 },
  { name: "android/mipmap-xxxhdpi/ic_launcher_foreground.png", size: 192 },
];

// Brand accent color — #4ec9b0 (eldritch teal)
const ACCENT_COLOR = { r: 78, g: 201, b: 176 };

async function createAppIcon(size) {
  // Logo fills 100% of the icon — no padding
  const logoSize = size;
  const padding = Math.round((size - logoSize) / 2);

  // Source logo is black strokes on TRANSPARENT background (alpha channel defines the shape).
  // We want teal strokes on dark background.
  // Strategy: use the alpha channel as the shape mask, paint it teal.
  const { data, info } = await sharp(LOGO_SRC)
    .resize(logoSize, logoSize, { fit: "contain", background: { r: 0, g: 0, b: 0, alpha: 0 } })
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });

  // Build RGBA buffer: where source has alpha (the strokes), paint accent teal
  const rgba = Buffer.alloc(info.width * info.height * 4);
  for (let i = 0; i < info.width * info.height; i++) {
    const a = data[i * 4 + 3]; // alpha from source — 255 = stroke, 0 = transparent
    rgba[i * 4]     = ACCENT_COLOR.r;
    rgba[i * 4 + 1] = ACCENT_COLOR.g;
    rgba[i * 4 + 2] = ACCENT_COLOR.b;
    rgba[i * 4 + 3] = a;
  }

  const logoLayer = await sharp(rgba, {
    raw: { width: info.width, height: info.height, channels: 4 },
  })
    .png()
    .toBuffer();

  // Create dark background and composite the colorized logo
  return sharp({
    create: {
      width: size,
      height: size,
      channels: 4,
      background: BG_COLOR,
    },
  })
    .composite([{ input: logoLayer, left: padding, top: padding }])
    .png()
    .toBuffer();
}

async function main() {
  console.log("Generating Cthulu app icons...\n");

  // Ensure output directories exist
  mkdirSync(BRAND_ASSETS, { recursive: true });
  mkdirSync(TAURI_ICONS, { recursive: true });
  mkdirSync(join(TAURI_ICONS, "ios"), { recursive: true });
  mkdirSync(join(TAURI_ICONS, "android/mipmap-mdpi"), { recursive: true });
  mkdirSync(join(TAURI_ICONS, "android/mipmap-hdpi"), { recursive: true });
  mkdirSync(join(TAURI_ICONS, "android/mipmap-xhdpi"), { recursive: true });
  mkdirSync(join(TAURI_ICONS, "android/mipmap-xxhdpi"), { recursive: true });
  mkdirSync(join(TAURI_ICONS, "android/mipmap-xxxhdpi"), { recursive: true });

  // Generate master 1024x1024 icon to brand assets
  const master = await createAppIcon(1024);
  const masterPath = join(BRAND_ASSETS, "app-icon-1024.png");
  writeFileSync(masterPath, master);
  console.log(`  ✓ ${masterPath}`);

  // Generate all Tauri sizes
  const allSizes = [...TAURI_SIZES, ...WINDOWS_SIZES, ...IOS_SIZES, ...ANDROID_SIZES];
  for (const { name, size } of allSizes) {
    const outPath = join(TAURI_ICONS, name);
    mkdirSync(dirname(outPath), { recursive: true });
    const buf = await createAppIcon(size);
    writeFileSync(outPath, buf);
    console.log(`  ✓ ${name} (${size}x${size})`);
  }

  // Generate .icns for macOS using iconutil
  console.log("\nGenerating macOS .icns...");
  const iconsetDir = join(TAURI_ICONS, "icon.iconset");
  mkdirSync(iconsetDir, { recursive: true });

  const icnsMap = [
    { name: "icon_16x16.png", size: 16, dpi: 72 },
    { name: "icon_16x16@2x.png", size: 32, dpi: 144 },
    { name: "icon_32x32.png", size: 32, dpi: 72 },
    { name: "icon_32x32@2x.png", size: 64, dpi: 144 },
    { name: "icon_128x128.png", size: 128, dpi: 72 },
    { name: "icon_128x128@2x.png", size: 256, dpi: 144 },
    { name: "icon_256x256.png", size: 256, dpi: 72 },
    { name: "icon_256x256@2x.png", size: 512, dpi: 144 },
    { name: "icon_512x512.png", size: 512, dpi: 72 },
    { name: "icon_512x512@2x.png", size: 1024, dpi: 144 },
  ];

  for (const { name, size, dpi } of icnsMap) {
    const buf = await createAppIcon(size);
    // Set DPI metadata — @2x icons must be 144 DPI for macOS
    const withDpi = await sharp(buf)
      .withMetadata({ density: dpi })
      .png()
      .toBuffer();
    writeFileSync(join(iconsetDir, name), withDpi);
  }

  execSync(`iconutil -c icns -o "${join(TAURI_ICONS, "icon.icns")}" "${iconsetDir}"`);
  // Clean up iconset
  execSync(`rm -rf "${iconsetDir}"`);
  console.log("  ✓ icon.icns");

  // Generate .ico for Windows (use the 256px version — .ico supports multi-res but
  // sharp can only do single-res, and 256 is the standard Windows icon size)
  const ico256 = await createAppIcon(256);
  writeFileSync(join(TAURI_ICONS, "icon.ico"), ico256);
  console.log("  ✓ icon.ico (256x256 PNG)");

  // Also copy logo to brand assets for sharing
  const logoBuf = await sharp(LOGO_SRC).toBuffer();
  writeFileSync(join(BRAND_ASSETS, "cthulu-logo.png"), logoBuf);
  console.log(`\n  ✓ Copied logo to ${join(BRAND_ASSETS, "cthulu-logo.png")}`);

  // Generate a favicon.png for the studio
  const favicon = await createAppIcon(32);
  writeFileSync(join(ROOT, "cthulu-studio/dist/favicon.png"), favicon);
  // Also put one in public/ for dev server
  mkdirSync(join(ROOT, "cthulu-studio/public"), { recursive: true });
  writeFileSync(join(ROOT, "cthulu-studio/public/favicon.png"), favicon);
  console.log("  ✓ cthulu-studio favicon.png");

  console.log("\nDone! All icons generated.");
}

main().catch((err) => {
  console.error("Failed:", err);
  process.exit(1);
});
