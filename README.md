# LEGO Legends of Chima: Laval's Journey HD textures for Vita3K

Every texture in the PS Vita game, upscaled 4x, as a texture replacement pack for the
[Vita3K](https://vita3k.org) emulator. Characters, levels, menus, icons: all 3,223 textures the
game ships with, not only the ones seen while playing.

- **Game:** LEGO Legends of Chima: Laval's Journey, **PCSE00237** (North America), version 1.00
- **Textures:** 3,223, upscaled 4x with Upscayl's *digital-art-4x* model (the 61 smallest, a few
  pixels across, resized smoothly instead)
- **Size:** about 3.7 GB, in three downloads

## Install

1. Download all three parts from the [Releases](../../releases) page.
2. Unzip each one into Vita3K's `textures\import` folder. They all unzip into the same
   `PCSE00237` folder, so you end up with:

   ```
   Vita3K\textures\import\PCSE00237\0000...png  (3,223 files)
   ```

   Vita3K's folder is the one you chose when setting it up (by default
   `%APPDATA%\Vita3K\Vita3K`). If `textures\import` isn't there yet, create it.
3. In Vita3K, right-click the game > **Custom Config > Create**, and on the **GPU** tab turn on
   **Import Textures**.
4. Start the game. Vita3K reads the pack when the game starts.

Any texture you'd rather keep as it was: delete its file, and the game uses the original again.

### Recommended settings

On the same GPU tab:

- **Internal resolution:** 3x or 4x (the Vita renders at 960x544; the textures are made for
  more than that)
- **Anisotropic filtering:** 16x, so floors and walls stay sharp at an angle

The biggest textures are 4096x4096, so expect a longer first load and short pauses entering new
areas while they load.

## Other regions

Vita3K names each replacement after a hash of the texture's data, so the pack works with any
release whose textures are identical to PCSE00237's. If yours is a different release and the pack
doesn't load, make one for it with the tools below.

## How it was made

The tools in [`tools/`](tools) read the textures straight from the game, so nothing has to be
collected by playing:

- **`unpack`** opens the game's `lego_chima_psp2.fib` (TT Fusion's FUSE1.00 archive). Files are
  stored in chunks compressed with TT's variant of RefPack. Every one unpacks to exactly the size
  in the archive's index.
- **`extract`** decodes the textures (DXT1 and DXT5 in the Vita's swizzled block order, and 32-bit
  ARGB) and names each the way Vita3K does: XXH3 of the texture's data XOR its size and format.
  Checked against textures Vita3K exported itself while playing: identical, all of them.
- **[`upscale.ps1`](upscale.ps1)** upscales a folder of textures with Upscayl's upscaler, one at a
  time, and checks each result (four times the size, same colours and transparency), with a
  smooth resize when the model can't handle a texture.

### Make the pack yourself

You need the game's files, [Rust](https://rustup.rs) and [Upscayl](https://upscayl.org).

```
cd tools
cargo build --release
target\release\unpack.exe  "<Vita3K>\ux0\app\PCSE00237\lego_chima_psp2.fib"  unpacked
target\release\extract.exe unpacked  "<Vita3K>\textures\export\extracted"
```

Then copy `upscale.ps1` into Vita3K's `textures` folder and run it. It upscales everything in
`textures\export` into `textures\import`:

```
powershell -ExecutionPolicy Bypass -File upscale.ps1
powershell -ExecutionPolicy Bypass -File upscale.ps1 -Model ultrasharp-4x
```

Any model in Upscayl's `resources\models` folder works; `-Watch` keeps it running and upscales new
textures as Vita3K exports them.

## Credits

- The original game and its artwork: TT Fusion, TT Games, Warner Bros. Interactive and the LEGO
  Group. This is an unofficial fan project, not affiliated with or endorsed by any of them, and it
  isn't for sale.
- [Upscayl](https://github.com/upscayl/upscayl) and its digital-art-4x model.
- [Vita3K](https://github.com/Vita3K/Vita3K), whose texture replacement makes this possible.
- The ZenHax community's notes on TT's FUSE archives.

The tools and script are MIT licensed (see [LICENSE](LICENSE)). The textures are upscaled from the
game's artwork and stay its owners'.
