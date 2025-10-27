# Application Icon Configuration

This project includes application icon support for Windows, macOS, and Linux.

## Icon Files

- **images/icon.png** - Source icon file (PNG format)
- **images/icon.ico** - Windows icon file (ICO format, auto-generated from PNG with white background)

## Platform Support

### Windows
The Windows executable embeds `images/icon.ico` at build time via the `build.rs` script using `winres`. The icon will be visible in:
- Windows Explorer file listings
- Task Manager
- Taskbar when the application runs
- Application shortcuts

### macOS
For macOS application bundles, the icon is configured in `Cargo.toml` under `[package.metadata.bundle]`. When building a macOS app bundle using `cargo-bundle`, the PNG icon will be automatically converted to an `.icns` file and included in the bundle.

To create a macOS app bundle:
```bash
cargo install cargo-bundle
cargo bundle --release
```

### Linux
Linux doesn't typically embed icons in executables. Instead, icons are provided via:
- **Desktop entry files** (`.desktop`) that reference the icon path
- **Package managers** (Snap, Flatpak, AppImage) that include icon metadata
- The PNG icon can be installed to standard locations like `/usr/share/icons/hicolor/256x256/apps/`

For Snap packages, the icon is already configured in `snapcraft.yaml`.

## Build Process

The `build.rs` script automatically:
1. Detects when building for Windows
2. Uses `winres` to embed `images/icon.ico` into the executable
3. Compiles the icon resource into the binary

No manual steps are required - just run `cargo build` or `cargo build --release`.

**Note:** The ICO file uses a white background instead of transparency to prevent black appearance when zooming in Windows Explorer.

## Updating the Icon

To update the application icon:
1. Replace `images/icon.png` with your new icon (recommended: 256x256 or larger, PNG format)
2. Regenerate the Windows ICO file with multiple sizes (16, 32, 48, 64, 128, 256):
   ```powershell
   # On Windows with PowerShell - Creates multi-resolution ICO:
   Add-Type -AssemblyName System.Drawing
   $pngPath = "images/icon.png"
   $icoPath = "images/icon.ico"
   $sourceImage = [System.Drawing.Image]::FromFile($pngPath)
   $sizes = @(256, 128, 64, 48, 32, 16)
   $bitmaps = @()
   
   foreach ($size in $sizes) {
       $bitmap = New-Object System.Drawing.Bitmap($size, $size)
       $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
       $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
       $graphics.DrawImage($sourceImage, 0, 0, $size, $size)
       $graphics.Dispose()
       $bitmaps += $bitmap
   }
   
   $memoryStream = New-Object System.IO.MemoryStream
   $header = [byte[]](0, 0, 1, 0, $sizes.Count, 0)
   $memoryStream.Write($header, 0, $header.Length)
   $currentOffset = 6 + ($sizes.Count * 16)
   $imageDataList = @()
   
   foreach ($bitmap in $bitmaps) {
       $imageStream = New-Object System.IO.MemoryStream
       $bitmap.Save($imageStream, [System.Drawing.Imaging.ImageFormat]::Png)
       $imageData = $imageStream.ToArray()
       $imageStream.Dispose()
       $imageDataList += $imageData
       $width = if ($bitmap.Width -eq 256) { 0 } else { $bitmap.Width }
       $height = if ($bitmap.Height -eq 256) { 0 } else { $bitmap.Height }
       $entry = [byte[]](
           $width, $height, 0, 0, 1, 0, 32, 0,
           0, 0, 0, 0, 0, 0, 0, 0
       )
       $size = $imageData.Length
       $entry[8] = $size -band 0xFF
       $entry[9] = ($size -shr 8) -band 0xFF
       $entry[10] = ($size -shr 16) -band 0xFF
       $entry[11] = ($size -shr 24) -band 0xFF
       $entry[12] = $currentOffset -band 0xFF
       $entry[13] = ($currentOffset -shr 8) -band 0xFF
       $entry[14] = ($currentOffset -shr 16) -band 0xFF
       $entry[15] = ($currentOffset -shr 24) -band 0xFF
       $memoryStream.Write($entry, 0, $entry.Length)
       $currentOffset += $imageData.Length
   }
   
   foreach ($imageData in $imageDataList) {
       $memoryStream.Write($imageData, 0, $imageData.Length)
   }
   
   [System.IO.File]::WriteAllBytes($icoPath, $memoryStream.ToArray())
   $memoryStream.Dispose()
   foreach ($bitmap in $bitmaps) { $bitmap.Dispose() }
   $sourceImage.Dispose()
   ```
3. Clean and rebuild the project: `cargo clean --release && cargo build --release`

## Dependencies

- **winres** (build dependency) - Windows resource compiler for Rust
