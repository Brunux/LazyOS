# System optimization for google-asurada (MT8192, 4 GB)

This port compiles its own kernel (`kernels/mediatek-mt81`, `scripts/build-kernel.sh`),
which means the kernel *configuration* is a knob we own — not something a distro
hands us. This file records what the device actually does today, and the changes
that are worth making because of it.

Every number marked *(measured)* was read from the device (the eMMC install,
kernel 7.2.7-mt81, `cat /proc/...`, `systemd-analyze`, `/sys/...`). The rest are
config facts read from `kernels/mediatek-mt81/config.aarch64`. Where the running
host differs from what `uis/sway` ships (the host is an older Omarchy-era
install), that is said explicitly — it is the difference between "this device"
and "this image".

Items are ordered by (effect × safety) / effort.

## Status

**In this batch**: 1 (MGLRU), 2 (`schedutil`), 3 (`HZ=1000`), 4 (`PREEMPT_DYNAMIC`),
11a (sysctl drop-in, written by `rootfs-setup.sh`), 11b (`systemd-zram-generator`
replaces `zram-tools`; the swap is now a `dev-zram0.swap` unit), 11c
(`systemd-oomd`), 11d (measured in section 12: `lz4` stays, and the cap moves from
the packaged `ram / 2` to two thirds of RAM; `zstd` remains the alternative to
measure), 12
(`transparent_hugepage=madvise` on the command line),
13 (kyber udev rule). `check-system.sh` grew a "System tuning" section that fails
or warns on each of them, because the failure mode here is silent:

**Next**: 5 (kernel zstd), 6 (thin LTO), 9 (dead options) — one card test.
**Optional**: 7 (zswap needs `ZPOOL`/`FRONTSWAP` first), 8 (sched_ext).
**Left out on purpose**: 14 (IRQ affinity). Pinning every IRQ to CPU0 may be the
deliberate low-power choice on this SoC — it is what ChromeOS does — so it needs a
before/after measurement of both latency and idle drain, not an assumption. It
stays in the list as something to measure, not something to ship.

## 1. What the device does today

### CPU

| | |
|---|---|
| Clusters | `policy0` 4×A55, 500 MHz - 2.0 GHz, capacity 388. `policy4` 4×A76, 774 MHz - 2.2 GHz, capacity 1024 *(measured)* |
| Governor | **`ondemand`** (Debian's `CONFIG_CPU_FREQ_DEFAULT_GOV_ONDEMAND=y`); `schedutil` is compiled in and available |
| Energy-aware scheduling | **off**: the DT has `capacity-dmips-mhz` (capacities are parsed) but **no `dynamic-power-coefficient`**, so no energy model registers and EAS cannot activate |
| AMU / PSI / uclamp | `CONFIG_ARM64_AMU_EXTN=y`, `CONFIG_PSI=y`, `CONFIG_UCLAMP_TASK=y` *(measured: `/proc/pressure/{cpu,io,irq,memory}` exist)* |
| `time_in_state` *(measured)* | little: 81 134 ticks at 2.0 GHz and a thin tail below; big: 142 675 at 2.2 GHz, **0** at 2.1/1.98/1.93 GHz |

`ondemand` ramps to the maximum and stays there. That is the classic signature in
the last row: with `schedutil` (and the AMU the driver has) the clusters spend
their time at the frequency the work actually needs. This is the difference
between battery and no battery on a 2-cluster Chromebook, and it costs one
config line.

`CONFIG_HZ=250` (`CONFIG_HZ_250=y`): input and scheduling latency for a desktop
are better at 1000. `CONFIG_PREEMPT=y` is already the full-preemption model.

### Memory

| | |
|---|---|
| RAM | 3 896 816 kB total, ~1.0 GB available under the load this was read at *(measured)* |
| Swap | `zram0` 1 948 156 kB (`PERCENT=50`), priority 100, **1.4 GB in use** *(measured)* — on the host the active algorithm is `lzo-rle`, in the image `ALGO=lz4` |
| THP | `[always]` |
| `swappiness` | 60 (Debian default) in the image; the host has 150, from Deboma's `/etc/sysctl.d/99-omarchy-sysctl.conf` |
| dirty limits | host: `dirty_bytes=256 MiB`, `dirty_background_bytes=64 MiB`; **the image ships no sysctl drop-in at all** |
| MGLRU | **off** — `CONFIG_LRU_GEN` is not set |
| zswap | `CONFIG_ZSWAP=y` but `CONFIG_ZPOOL`/`CONFIG_FRONTSWAP` are **not**, so zswap cannot be enabled: the option is inert |

### Storage and interrupts

| | |
|---|---|
| I/O scheduler | host: **kyber** (Deboma's `/etc/udev/rules.d/60-omarchy-io-scheduler.rules`); **the image has no scheduler rule**, so the eMMC gets `mq-deadline` (kernel default; both schedulers are compiled in) |
| `read_ahead_kb` / `nr_requests` | 128 / 32 (eMMC), 128 / 128 (card) |
| IRQ placement *(measured)* | everything on CPU0: eMMC `11f60000.mmc` 1 024 210, i2c `11d21000.i2c` 351 792, WiFi `rtw88_pci` 376 090, and `irqbalance` is not installed |
| Modules | 30 MB (zstd-compressed) |

### Boot

`2.445 s kernel + 5.183 s userspace = 7.628 s` *(measured, `systemd-analyze`)*.
Userspace blame: `dev-mmcblk0p3.device` 1.6 s, `systemd-udev-trigger` 1.2 s,
`systemd-journald` 0.9 s, `upower` 0.76 s, **`depthcharge-tools.service` 0.66 s**,
`systemd-binfmt` 0.6 s, `rtkit-daemon` 0.55 s.

**Do not mask `depthcharge-tools.service`.** It runs `depthchargectl bless`, which
marks the ChromeOS kernel partition successful and resets its `tries` counter
(`-S 1 -T 5 -P 10` in `lib-cgpt.sh`). Without it the firmware burns one try per
boot and after five the kernel partition is no longer bootable. The image ships
and enables it (`/etc/systemd/system/basic.target.wants/`), and that 0.66 s is the
price of the boot chain. `systemd-binfmt` (0.6 s) is only needed for foreign-arch
binaries (`qemu-user`); everything else on the list is a few hundred ms each and
worth more than it costs.

## 2. Kernel configuration (we compile it, so these are ours)

| # | Option | Today | Propose | Why / cost | Risk |
|---|---|---|---|---|---|
| 1 | `CONFIG_LRU_GEN` + `CONFIG_LRU_GEN_ENABLED` | off | **on** | Multi-generational LRU: reclaim decides by generation instead of scanning everything, which is exactly the 4 GB-under-zram case (ChromeOS ships it for this). Runtime knob stays at `/sys/kernel/mm/lru_gen/enabled`. | low; new MM default, but it is the kernel's own upstream feature |
| 2 | `CONFIG_CPU_FREQ_DEFAULT_GOV_SCHEDUTIL` | `ondemand` | **`schedutil`** | Frequency follows scheduler utilization + AMU instead of `ondemand`'s sampling ramp-to-max (see the `time_in_state` row). Also the prerequisite for EAS (item 12). Runtime alternative: a systemd rule writing the governor, no rebuild. | low |
| 3 | `CONFIG_HZ_1000` | `HZ_250` | **1000** | Desktop latency: timer granularity 4 ms → 1 ms. Costs a little idle power; with `NO_HZ_IDLE` the tick is off when idle anyway. | low |
| 4 | `CONFIG_PREEMPT_DYNAMIC` | not set | **on** | Keeps `PREEMPT=y` as the default but lets `preempt=full|voluntary|none` be chosen on the command line, so the model can be A/B tested without a rebuild. | low |
| 5 | `CONFIG_KERNEL_ZSTD` | unset (kernel **Image is uncompressed, 15.6 MB**) | **on** | The kpart is 29 MB, most of it kernel + initramfs; a compressed `Image` (~6 MB) shrinks it and gives depthcharge less to load. It also frees the compressed copy at boot. | **needs a card test**: `scripts/make-kpart.sh` reads `/boot/vmlinuz-$release` and depthcharge has to boot the self-decompressing `Image`. Confirm what `make zinstall` installs first. |
| 6 | `CONFIG_LTO_CLANG_THIN` | `LTO_NONE` | **on** | The build is already `LLVM=1`, so thin LTO is a config line: better cross-file inlining (a few % in hot paths) and a smaller image, at ~10-20 % build time. | medium: a miscompile would show as a boot failure; one card test covers it |
| 7 | `CONFIG_ZPOOL` + `CONFIG_FRONTSWAP` | off | on **only if** zswap is wanted | Today `CONFIG_ZSWAP=y` does nothing. With these, zswap can sit in front of a disk swap (better ratio, writeback to eMMC). With zram-only there is no need. | low |
| 8 | `CONFIG_SCHED_CLASS_EXT` | off | optional | Enables sched_ext, i.e. trying a `scx_*` scheduler (e.g. `scx_lavd`, built for handhelds) without patching the kernel. `CONFIG_BPF_JIT=y` is already on. Needs the `scx` userland, which is not packaged. | medium: an external scheduler can misbehave; drop it if it does |
| 9 | Dead options | `ARM64_SVE`, `ARM64_MTE`, `XEN_DOM0`, `SCHED_SMT`, `SLUB_DEBUG` on | drop | None of this hardware has SVE or MTE, there is no Xen domain, and ARM has no SMT. Small size/build-time win only. **Keep `KALLSYMS_ALL`** — crash symbolization is worth its few MB. | low |
| 10 | Security/perf | `HARDENED_USERCOPY`, `FORTIFY_SOURCE`, `ARM64_SSBD` **already off** | leave | There is nothing left for `mitigations=off` to disable, so the usual "free 5 %" does not exist here. | — |

## 3. Image configuration (no kernel rebuild needed)

| # | What | Today | Propose | Why |
|---|---|---|---|---|
| 11a | swap policy | no sysctl drop-in; Debian `swappiness=60` | `vm.swappiness=150` (+ `vfs_cache_pressure=50`, `dirty_bytes=64 MiB`, `dirty_background_bytes=16 MiB`) | Swapping into compressed RAM is cheap, and 60 makes the kernel prefer dropping page cache instead. The dirty limits are the opposite of Deboma's 256/64 MB: on eMMC a large dirty window is where multi-second stalls come from when copying files. |
| 11b | one zram implementation | image: `zram-tools` (works, `OK swap active: /dev/zram0` in `reports/`) | keep `zram-tools`, or move to `systemd-zram-generator` | The host here has **both**, and `zramswap.service` fails on every boot (`Device or resource busy` → `mkswap: /dev/zram0 is mounted`) while systemd's generator does the work. Exactly one mechanism. `systemd-zram-generator` has one advantage: the swap becomes a real `dev-zram0.swap` unit, which is what `systemd-oomd` needs (11c). Its algorithm/size live in `/etc/systemd/zram-generator.conf`. |
| 11c | `systemd-oomd` | not installed | install + `ManagedOOMSwap=kill` | PSI is enabled, and on 4 GB the failure mode is "frozen machine", not "process killed". oomd kills a hog by pressure instead of letting the kernel thrash. Note the requirement in 11b: `ManagedOOMSwap=` matches swap *units*. |
| 11d | zram algorithm/size | `lz4`, 50 % (1.9 GB) | **`zram-size = min(ram * 2 / 3, 4096)`**, keep `lz4` (section 12) | Measured 4.4 : 1 in real RAM (0.23 GiB per GiB of fast tier), so the packaged 1.9 GB cap is left behind - but the full-RAM cap that was shipped first was dropped again: with `swappiness=150` a cap that never fills removes the backpressure that forces the free drop-cache path, so the machine swaps out cold pages and every re-touch pays an lz4 decompress (a real, observed latency). Two thirds of RAM keeps double the old headroom while zram stays a burst tier. `zstd` at the CPU is unearned until someone runs the A/B. |
| 12 | THP | `[always]` | `transparent_hugepage=madvise` on the command line | `always` + zram on 4 GB wastes memory (a 2 MB fault vs. compressed 4 KB pages) and adds latency spikes; only a handful of workloads want it. Runtime-testable via `/sys/kernel/mm/transparent_hugepage/enabled`. |
| 13 | I/O scheduler | `mq-deadline` in the image, `kyber` on this host | ship a udev rule (or `elevator=kyber`) | kyber is built for flash: low latency under mixed load instead of deadline's global reordering. Both schedulers are compiled in, so this is a runtime choice. |
| 14 | IRQ affinity | every IRQ on CPU0 | `irqbalance`, or explicit `smp_affinity` rules for `mmc`, `rtw88_pci`, the i2c EC | eMMC + WiFi + input interrupts all land on the same core the scheduler uses; moving them to another little core (or the big cluster) is free latency. `irqbalance` is packaged (253 kB). |

## 4. Research items

* **EAS with a DT patch (item 2's other half).** The kernel has `ENERGY_MODEL`
  and the DT has capacities, but no `dynamic-power-coefficient`, so no energy
  model registers and `sched_energy_aware` never appears. Since
  `kernels/mediatek-mt81/patches/` is ours, adding the property to the CPU nodes
  would enable EAS — if the coefficients are right. Wrong values make it worse
  than off. This is the battery-life item, and it needs measurement, not
  guesswork.
* **Hibernation.** `CONFIG_HIBERNATION=y`, `/sys/power/state` = `freeze mem disk`,
  `/sys/power/disk` = `[shutdown]`. It needs a disk swap the image does not have
  (see the swap discussion) plus `resume=` on the kpart's command line and
  `RESUME=` in `initramfs-tools`, and a hardware test — a failed resume burns the
  kernel partition's `tries`. Suspend-to-RAM already works and resumes cleanly.
* **Boot without an initramfs.** `MMC_MTK`, `EXT4_FS`, `DRM_MEDIATEK` and
  `PANFROST` are all built in, so the kernel can mount `root=UUID=…` itself. The
  initramfs is 19 MB of the 29 MB kpart and its decompression is part of boot
  time. Worth one experiment on the card; initramfs-tools is still needed for
  hibernation and for `update-initramfs` after a kernel install.
* **sched_ext** (item 8) and **initramfs-free boot** are both "try it on the card
  and measure", not commitments.

## 5. How to verify each change

```sh
cat /sys/devices/system/cpu/cpufreq/policy*/scaling_governor
cat /sys/devices/system/cpu/cpufreq/policy*/stats/time_in_state   # frequency, per state
cat /sys/kernel/mm/lru_gen/enabled                                # MGLRU
cat /sys/kernel/mm/transparent_hugepage/enabled                   # THP
cat /sys/block/zram0/comp_algorithm /proc/swaps                   # zram
cat /proc/pressure/memory                                         # PSI, before/after load
systemd-analyze ; systemd-analyze blame                           # boot
awk '{for (i=2;i<=NF-2;i++) s[i]+=$i} END {for (i in s) printf "cpu%d %d\n", i-2, s[i]}' /proc/interrupts
sudo cgpt show /dev/mmcblk0 | grep -A3 pmOS_kernel                 # tries/successful
```

Load to measure against: `stress-ng --vm 2 --vm-bytes 1G --timeout 60s`
(624 kB package) and `fio` (2.4 MB) for the eMMC. Boot-chain changes are only
proven by booting the card and running `scripts/check-system.sh` on it.

Two rules for this work, both learned from this port: **one risky change per
card boot** (a kernel that will not boot tells you nothing if three things
changed), and **never leave the kernel partition's `tries` counter short** —
`scripts/install-kpart.sh` writes a fresh `-T 5`.

## 6. Suggested order

1. **The no-rebuild batch**: 1 (MGLRU), 2 (schedutil), 3 (HZ), 11a-11d (zram,
   swappiness, oomd), 12 (THP), 13 (kyber), 14 (IRQs), 9 (dead options). One
   kernel rebuild + one card boot; several of them are runtime-testable first.
2. **The size/compression batch**: 5 (kernel zstd), 6 (thin LTO), 4
   (PREEMPT_DYNAMIC), tested on the card together but changed one at a time in
   the config.
3. **EAS** (DT patch) — battery, needs coefficients and measurement.
4. **Hibernation / swap layout**, **sched_ext**, **initramfs-free boot** — only
   if wanted.

## 7. Compositor effects: what blur costs, and which renderer

Added when blur was turned on in `uis/sway/files/etc/sway/config.d/26-swayfx.conf`. It is not a
kernel knob, but it is the same kind of decision: the effect is either affordable on this GPU or it
is not, and that is a measurement, not an opinion.

### The renderer this device has

* **GLES on panfrost, and nothing else.** swayfx never calls `wlr_renderer_autocreate()` - it
  creates SceneFX's renderer itself (`fx_renderer_create()` in `sway/server.c`), and SceneFX only
  implements a GLES renderer. `WLR_RENDERER` therefore does not reach it at all; there is no
  Vulkan, pixman or "try the other one" path for the session.
* **Vulkan is not available on this GPU anyway.** `vulkaninfo` with Mesa 26.1.6 answers
  `Mali-G57 not supported (VK_ERROR_INCOMPATIBLE_DRIVER)` from `panvk_physical_device.c`: PanVK has
  no support for the Mali-G57. The two facts agree, which is why there is nothing to tune here
  beyond making sure the GLES driver is the hardware one (it is: EGL driver name `panfrost`,
  `GL_RENDERER` "Mali-G57 MC5 (Panfrost)", GLES 3.1).
* **The active mode is 1366x912 at 60.004 Hz** (`wayland-info`; every number below is quoted at that
  resolution), not the 1920x1200 the old comment in `26-swayfx.conf` claimed. A frame is 16.7 ms.
* **What the image actually runs, checked rather than assumed.** greetd starts `sway` with no
  `WLR_*`, `MESA_*` or `LIBGL_*` override anywhere in the tree, so every knob is the default; the
  rootfs has `libegl-mesa0`, `libgl1-mesa-dri` and `mesa-libgallium` 26.1.6 with `panfrost_dri.so`
  installed and **no `mesa-vulkan-drivers` at all** (nothing would load it on this GPU anyway).
  `check-system.sh` now fails when `eglinfo -B` does not name Panfrost, because a forced
  `LIBGL_ALWAYS_SOFTWARE` or a Mesa regression would otherwise drop the session onto llvmpipe
  silently, with every frame and the blur drawn on the CPU.
* **The Mesa tuning variables that exist do not help.** `PAN_MESA_DEBUG` accepts `dump`, `trace`,
  `sync`, `inorder`, `nocache` and `precompile` - instrumentation, validation, and `inorder`/
  `nocache` which *cost* performance. There is no "make it faster" switch; the way to make this
  GPU faster is to draw less, which is what the two blur paths below are about.
* **Colour management is compiled in and switched off.** SceneFX's meson found `lcms2` and built
  the colour path, but `fx_renderer.c` sets `features.output_color_transform = false`, so no
  per-frame conversion happens and there is nothing to disable at runtime.

### The "Mali-G57 PanVK driver" is not a shortcut

Mesa's own matrix (docs.mesa3d.org/drivers/panfrost.html) leaves the Vulkan column empty for
*Valhall (v9) G57/G68*; Mesa main implements `panvk_v6`, `v7` and `v10`-`v14`, and the G57 is v9.
`PAN_I_WANT_A_BROKEN_VULKAN_DRIVER=1` does not change that - tested on the device, `vulkaninfo`
then enumerates only llvmpipe. The binary driver that circulates for this GPU
(FristOneRR/PanVK, 2026-10) cannot fill the gap:

* it is not a source project - the repository is a README and a license, the releases are a
  prebuilt 17.6 MB `libvulkan_panfrost.so`;
* that `.so` is built with the Android NDK ("for Android 24", minApi 33) and needs `liblog.so`,
  `libhardware.so`, `libnativewindow.so` and `libsync.so` - bionic libraries Debian does not have
  and cannot substitute at the Vulkan-loader level;
* it drives **kbase** (`/dev/mali0`, `KBASE_IOCTL_*`, gralloc); this kernel binds the GPU to
  **panfrost** (`/dev/dri/renderD128`, DRM uAPI - `/dev/mali*` does not exist here), so the
  interface it was written against is absent;
* **the source forks add v9 for kbase, not for panfrost.** `apexspan-svg/mesa-panvk-mali-g57` is a
  full Mesa tree with G57/v9 support and `zenithblue-oss/panvk-kbase-android` a standalone build;
  both say outright that they target the Arm `kbase` JM interface instead of mainline
  `panfrost.ko`. `Vtgamer998/PanVK-v9-Driver` even ships a **glibc** build
  (`zig cc -target aarch64-linux-gnu.2.35`), but it is a Winlator ICD - `pan_kmod_kbase.c` +
  `kbase_winsys.c` on `/dev/mali0`, XCB WSI - and its README calls it "not a complete driver"
  (286 stubbed extensions) that "may cause GPU JOB_READ_FAULT". Using any of them means replacing
  their kernel interface with Mesa's `pan_kmod` DRM backend: a Mesa driver project, not a package
  drop;
* even a working Vulkan ICD would not touch this section: swayfx creates SceneFX's **GLES**
  renderer, so the compositor and its blur would keep using panfrost either way. Vulkan would only
  matter to Vulkan applications.

If upstream PanVK grows Valhall v9 support on `pan_kmod`'s DRM backend, the answer becomes
`apt install mesa-vulkan-drivers` on forky - no port and no private driver.

### The two paths SceneFX has

SceneFX (0.5) classifies each blurred node:

* **Cached ("optimized") blur.** A tiled window that cannot overlap another one - it is neither
  floating nor animating - samples a buffer that was blurred once, when the wallpaper or the output
  size changed. Per frame that is one bilinear tap per pixel.
* **Live blur.** A floating window, or one that can overlap another, is re-blurred from what is
  actually behind it: `blur_passes` downsample passes (5 taps) and the same number of upsample
  passes (9 taps), then the full-screen draw of the result. `could_container_overlap()`
  (`sway/desktop/output.c`) answers true while the container or its workspace has an animation
  initialized, so an animating tiled window takes this path for the length of its animation.

`blur_xray` chooses between them: enabled, *every* surface uses the cached buffer - including a
floating window, which then shows the wallpaper through itself instead of the window behind it.
That is visible (verified with two screenshots of the same scene, one per setting), so the session
keeps it disabled and only floating and animating windows pay for a live blur.

### Measured

[`bench-blur.c`](bench-blur.c) is the harness: an EGL/GLES2 context on `renderD128`, SceneFX's own
`blur1.frag`/`blur2.frag` verbatim, `glFinish()` on both sides, best of five runs of 300 iterations,
at 1366x912:

```sh
gcc -O2 -o bench-blur docs/bench-blur.c -lEGL -lGLESv2 -lm && ./bench-blur 1366 912
```

| Path | Work per frame | Time |
|---|---|---|
| Cached, full screen | 1 bilinear tap of the cached texture | **0.07-0.11 ms** |
| Live, radius 4 / 2 passes, full screen | 2 down + 2 up passes + the final draw | **2.1-2.2 ms** |
| Live plus the post-effects pass (noise/tint) | one more full-screen pass | **+0.6 ms** |
| Shipped defaults (radius 5, 3 passes, effects) | two more passes than the session's setting | **3.0-3.1 ms** |
| Building the cached buffer | the same chain, once per wallpaper/resize | ~2.7 ms, once |

The live path scales with the window's area: a 1200x800 floating window is 69 % of the screen, so
about 1.5 ms. An end-to-end check agrees that this is small: a client repainting a 2600x1700
window in a headless swayfx held 60 fps with blur off, with the cached path, with the live path and
with SceneFX's defaults (radius 5, 3 passes, noise). The blur is not what limits this GPU at this
panel's resolution; it was never the frame-rate cliff the port's earlier notes assumed.

The parameters in `26-swayfx.conf` follow from the table:

* `animation_duration_ms 150` is the one animation setting swayfx 0.6 has: a single float, no
  per-event directives and one fixed ease-out-cubic, driving open/close (pop-in/out from 80 % size
  at the centre, with the alpha), every tiling move and resize, and the workspace cross-fade. It
  is also the price: `could_container_overlap()` counts an initialized animation as "can overlap",
  so each animating window takes the live path for those ~9 frames at 60 Hz (2.1-2.2 ms/frame
  full screen, 1.5 ms for 1200x800), and a workspace switch animates both workspaces. Interactive
  resizing and dragging a floating window are not animated (`seat_is_resizing`/
  `seat_is_moving_float` bypass it), so the pointer stays 1:1, and `swaymsg animation_duration_ms 0`
  restores the cached path at runtime.
* `blur_passes 2` + `blur_radius 4` = a 32 px sample (`2^(passes+1) * radius`; the shipped default
  is 5 x 3 = 80 px). Passes are the price of a live blur, and the sample size is also how far a
  *live* blur's damage region is grown, so this is what a floating window re-renders around itself.
* `blur_noise 0` with brightness/contrast/saturation at 1.0 makes SceneFX skip the post-effects
  pass entirely; it costs one full-screen pass on a live blur and adds visible grain. For the
  cached path the pass is baked in when the buffer is built, so `blur_brightness 0.9` is free
  there - it is the line to add if a dimmer frost is wanted.
* Blur is drawn *behind* a surface, so it is only visible under one with alpha < 1. The three
  carriers are `~/.config/foot/foot.ini` (`alpha=0.92`), `~/.config/rofi/solarized.rasi` (0.88) and
  `~/.config/waybar/style.css` (0.78 for the bar, 0.86 for the cards). The bar is a layer surface
  that only ever has the wallpaper behind it, so `layer_effects "waybar"` uses the cached path; the
  launcher floats over windows, so `layer_effects "rofi"` uses the live one (and only repaints
  while open).
* Blur does not disable direct scanout by itself: `has_blur` is consulted only to render the cached
  buffer and to compensate damage, never in `scene_entry_try_direct_scanout`, so an opaque
  fullscreen buffer can still be scanned out with a blur node underneath it. What does cost scanout
  is the translucency that makes the blur visible - an `alpha < 1` surface is composited. That is
  the deliberate trade for the frosted look, and it is cheap in practice because the compositor is
  damage-driven: an idle translucent terminal produces no frames at all.

`vulkaninfo`, `eglinfo` and `wayland-info` give the facts that bound the options above. One detail the
harness had to get right, and a naive version gets wrong: every pass must read and write *different*
buffers. Sampling the target you are writing is a feedback loop, which the driver may legally throw
away - a first version of it measured a chain that cost less than one of its own passes.

## 8. The file browser's thumbnails

The file browser (`rofi-files`) shows a 32 px picture on every row that has one. It is the only
thing in the session that decodes media, so the question was not "does it work" but "what does a
screenful cost", and the answer decides three settings: how many decoders run at once, how they are
threaded, and how a JPEG is opened. The numbers below were measured on the build host (aarch64, the
same SoC) with forky's rofi 2.0.0-0.2 and this host's ffmpeg 9.0.2, driven by `-preview-cmd` exactly
as the session runs it; the card was not booted, so treat them as the shape of the cost rather than
as a promise for forky's ffmpeg 8.x (same decoders, a little slower).

### What rofi does on its own

* rofi draws a row's icon from a `\0icon\x1f<value>` suffix. A `thumbnail://<path>` value makes it
  call the `-preview-cmd` for that row and keep the result in
  `~/.cache/thumbnails/normal/<md5 of the path>.png`, and **it skips the call when the file is
  already there** (two runs of the same listing: one call). The cache is rofi's, so the generator
  does not have to think about reuse at all.
* **It is lazy and parallel already.** rofi asks only for the rows it is about to draw - ten of
  forty, roughly - and starts one process per row at the same time: 8 preview processes within
  30 ms. Scrolling asks for the next screenful only. That is the "generate at scrolling time"
  behaviour, and it is why the script never needs a warm-up pass.
* Consequence for the generator: the parallelism is **across files**, so `-threads 1
  -filter_threads 1` per decoder is right - ffmpeg's own thread pool would take the other decoders'
  cores and make the screenful finish later, not sooner.

### What a screenful costs

| Case | Time | Peak RSS |
|---|---|---|
| 1280x800 PNG, one decode | 54 ms | 50 MB |
| 24 MP PNG, one decode | 577 ms | 131 MB |
| 24 MP JPEG, default decode | 179 ms | 83 MB |
| 24 MP JPEG, `-lowres 2` | **53 ms** | **50 MB** |
| PDF page 1, `pdftocairo -scale-to 128` | 132 ms | 22 MB |
| Ten 1280x800 PNGs, one after another | 2.0 s | - |
| Ten 1280x800 PNGs, eight requests, four slots | **0.97 s** | - |
| Ten 24 MP JPEGs, one after another | 1.8 s | - |
| Ten 24 MP JPEGs, eight requests, four slots | **0.93 s** | - |

Time is `ffmpeg -benchmark`'s `rtime` and the wall clock of the whole loop; RSS is `VmHWM` sampled
from `/proc/<pid>/status` while the decode ran, which is the number that decides how many decoders
can run at once.

The fan-out that makes it fast is also what would make it heavy: eight 24 MP PNGs at once is
~1 GB on a 4 GB machine. `lazyos-thumbnail` therefore gates itself to **four** decoders with `flock`
slots (measured: max 4 concurrent ffmpeg over 8 requests), which is ~200 MB for the realistic case
(12 MP phone JPEGs, ~50 MB each at `-lowres 2`) and finishes a screenful in about a second. `flock`
releases the slot when the process dies, so a killed listing cannot leave the browser waiting on a
lock nobody holds - the reason for a lock file and not a pid counter. The fifth and later requests
wait on the slot their path hashes to, which spreads them over the four instead of queueing all of
them on the first one (1.1 s against 0.97 s for ten files).

### What the browser itself costs

PSS of the `rofi` process, read from `/proc/<pid>/smaps_rollup` with the window open, on the build
host with forky's rofi 2.0.0-0.2:

* rofi in dmenu mode, thirteen text rows: **17.1 MB** (17.1 and 17.1 on two runs).
* the same window with fourteen rows of 32 px thumbnails: **23.2 MB**. The delta is rofi's icon
  cache - it keeps the decoded pixbufs for the rows it has drawn and drops them with the window
  (nothing stays after a selection, and no separate process exists at any point).
* the launcher's application icons, for comparison: **13.2 MB** for ten rows. A `.desktop` icon is
  48 px and comes from a theme directory, a thumbnail is 128 px decoded from a photo, which is why
  the file browser's number is larger and why the 32 px is per menu rather than a theme-wide
  change (the launcher and the bar popups keep 26 px).
* the viewer: **imv** draws the picture on the GPU in its own window, and the fallback
  (`foot --hold` + `chafa -f sixels`) is **16 MB** of PSS for a 6000x4000 photo - that number is
  foot's, not chafa's (chafa draws once and exits, and the sixel picture stays in the terminal's
  scrollback). A browser window for the same file is hundreds of MB (the package is 280 MB
  installed), which is the whole reason pictures no longer go there.

### The rules that follow

* **Four decoders, not eight.** `SLOTS=4` in `lazyos-thumbnail`; the constant is the knob if a bigger
  folder ever feels slow, and it is bounded by the worst decode (131 MB for a 24 MP PNG) rather
  than the average.
* **`-lowres 2` for JPEG only.** It is a quarter-size decode in one step (53 ms against 179 ms and
  50 MB against 83 MB at 24 MP); the PNG/TIFF decoders ignore the option, so it is applied by
  extension and not globally.
* **The 128 px is rofi's**, not the row's: rofi always asks for 128 and the theme scales it to 32,
  so the cache is shared with any other 128 px consumer and the file stays 3-8 kB.
* **Atomic writes.** The generator writes `<output>.part.<pid>.png` and renames. rofi reads the
  cache path directly, and a decode killed by a closing window would otherwise leave a truncated
  PNG that looks cached forever - a wrong thumbnail that never regenerates.
* **No thumbnailer package.** `-preview-cmd` replaces the XDG thumbnailer protocol, so no
  gdk-pixbuf loader, glycin or ffmpegthumbnailer package is installed for this; the two additions
  are `poppler-utils` (1.1 MB, for the PDF page) and `chafa` (0.7 MB with `libchafa0t64`, for the
  picture fallback when imv gets no graphics context). `ffmpeg` was already in the image for
  `rofi-color` and dictation.

## 9. The terminal file search

`Search+Shift` runs `~/.config/bin/lazyos-find`, which is `fd` piped into `fzf` with a preview pane.
It answers the same question the file list (`Alt+Search`) does, in a different way: `rofi-files --search` is
bounded (500 rows, 92 ms end to end, icons and thumbnails) and lives in a menu, while this one is
uncapped with a preview and lives in a terminal. The numbers below decide the settings that are not
obvious - the excludes, `--scheme=path`, and where the picture comes from - and were measured on the
build host (aarch64, same SoC) with forky's fd 10.5.0 and fzf 0.74.4, driven inside a pty so fzf
painted at the size it paints in a real foot window.

### What the walk costs

`fd --type f --hidden --strip-cwd-prefix` over this `$HOME` (348k files, 330k of them under
`.cache`), wall clock of the walk alone, ten runs:

| Pool | Files | Time |
|---|---|---|
| visible only, no excludes | 2,020 | 21 ms |
| hidden + the four excludes | **19,400** | **43 ms** |
| everything, hidden included | 348,000 | 1.6 s |

The excludes are `.cache`, `.git`, `node_modules` and `.local/share/containers`, the same four
`rofi-files --search` uses - and `.cache` alone is why the third row exists: an unfiltered walk is
37 times the work for a list where every seventh row is a browser cache entry.

### What the list costs

fzf, from the first byte of `fd`'s output to the first painted list, and from a keystroke to the
repainted list (the `change` event), PSS from `/proc/<pid>/smaps_rollup` with the window open:

| Case | Time | PSS |
|---|---|---|
| first paint, 19.4k rows | **24 ms** (17-24) | - |
| keystroke repaint, `--scheme=path` | **5-35 ms** | - |
| window open, 19.4k rows | - | **35.6 MB** |
| window open, 348k rows | - | **86.7 MB** |

fzf filters **as the producer streams**: the prompt is up and typable before `fd` has finished, so
the 43 ms above and the 24 ms here overlap instead of adding up. `--read0`/`--print0` keep a name
with a space, a tab or a newline one row and one argument; `--scheme=path` ranks a path by its
segments, which is what makes typing `sway conf` reach `~/.config/sway/config` (the default scheme
scores the whole string and prefers a shorter candidate).

The 86.7 MB row is the reason the excludes are not optional: fzf keeps a scored entry per row, and
348k of them turns a search window into the heaviest thing in the session, for a list where every
seventh row is a browser cache entry.

### The preview

The pane is 55 % of the width and shows one of three things: the first 300 lines of a text file
through `bat` (`--paging=never`, `--style=numbers`, `BAT_THEME=ansi`), a picture or a PDF page, or
`file`'s mimetype plus `du` for anything else. A preview is drawn **for the row under the cursor
only**, so the cost is one decode of the 128 px thumbnail `lazyos-thumbnail` already keeps for
`rofi-files`: the same cache path (`~/.cache/thumbnails/normal/<md5>.png`), the same generator, and
a picture is decoded once ever, whichever of the two asked for it.

The pane draws with **`chafa -f symbols`**, not sixels, and that is the one setting that could be
mistaken for a downgrade. Sixels are what the file viewer uses (`foot --hold -e chafa -f sixels`)
because there chafa owns the terminal and nothing repaints over it. Here the pane is a region of
fzf's own terminal: every keystroke redraws the screen, and a sixel image - which is placement
state in the terminal, not text - is erased or smeared by that redraw. A symbols image is ordinary
text, so it survives. The cost of the difference is resolution: ~80x24 cells for a picture, against
the full window in the viewer, and the 128 px thumbnail is already about that.

### The alternatives

* **fzf** (chosen). Already in `packages/tools.list` for the shell's Ctrl-R/Ctrl-T/Alt-C bindings
  and `ff`, so the feature adds a script and no package; filters as the producer streams;
  `--read0`/`--print0` are path-safe; the preview is a setting, not an architecture.
* **skim** (`sk`). A Rust clone of fzf with the same interface and the same feature set for this
  job. It would be a second fuzzy finder in the image doing what the first one does, and nothing
  measured made it faster here - rejected on the duplication, not on quality.
* **television** (`tv`). Nice channel/syntax model, and not packaged in forky: it would need a
  cargo build on the build host and a vendored dependency tree for a tool that has to be on the
  first boot. `fd | fzf` is the same pipeline with two packaged binaries.
* **fzy**. Simpler, no preview pane and a slower scorer on the 19.4k-file case; the preview is the
  reason this search exists next to `rofi-files`.
* **fzf's own `--walker`**. It walks without `fd`, which sounds like one process fewer, but it does
  not read `.gitignore`: vendor and build directories come back and the list is no longer the tree
  the person meant. `fd` in front keeps one definition of the pool.

The rules that follow are the four traps, each of them the reason a line in the script exists:
`--strip-cwd-prefix` (fd's `--print0` prefixes `./`, and the prefix reached both the rows and the
opener), `--no-multi` (the opener takes one file), `-f symbols` (above) and `--hidden` with the
excludes (the 2,020-to-19,400 jump is the whole point of asking for the whole tree).

## 10. The idle screen

Deboma's screensaver is `omarchy-screensaver`: a fullscreen terminal looping `ttfx` at 120 fps for as
long as the machine is idle. Measured on this host, that loop is 78 MB of Python and 74-86 % of one
core on a 200x60 canvas, which is what the battery notices. This port keeps the animation and changes
what it costs: Debian's `tte` at **30 fps**, looping the seven curated effects over
`~/.config/lazyos/screensaver.txt` with `--reuse-canvas`.

### Method

Real art, a real pty (`pty.fork`, 80x24, then resized to 40x100 like a foot window), and `script -c`
around a shell that runs each effect once in the shipped order. CPU is `getrusage`'s user+sys for the
whole tree (bash's `time` accumulates each `tte` child), so Python start-up is included; memory is the
tallest `VmHWM` any `tte` process reaches. Nothing else runs on the machine during the measurement.

One *cycle* below is each effect once, in the order `lazyos-screensaver` draws them.

### What a cycle costs at 30 fps

| Effect | Wall | CPU | Share while it plays |
|---|---|---|---|
| `decrypt` | 32.0 s | 2.99 s | 9 % |
| `middleout` | 4.1 s | 0.91 s | 22 % |
| `print` | 17.3 s | 1.09 s | 6 % |
| `sweep` | 8.0 s | 1.97 s | 25 % |
| `wipe` | 5.0 s | 0.55 s | 11 % |
| `slide` | 4.5 s | 0.79 s | 18 % |
| `expand` | 3.0 s | 0.67 s | 22 % |
| **cycle** | **70.7 s** | **8.47 s** | **12 %** |

So the screen costs about an eighth of one core while it is up, peaks at **49 MB** of `tte` RSS on
top of foot's ~25 MB, and costs nothing at all once it is dismissed or the lock takes over. The share
is not the same for every effect: `decrypt` is the longest and the cheapest per second, `sweep` and
`middleout` are the heaviest.

### The frame rate is the whole cost

`tte`'s effects have a fixed frame count, so the frame rate buys smoothness and pays for it in rate:
the same cycle at **120 fps** is over in 20.2 s but spends 6.42 s of CPU, i.e. **32 % of one core**.
30 fps is the point where the animation still moves (a cycle is about a minute) for a third of that.

The temptation is to copy Deboma's 120 and let the numbers be someone else's problem; the two
measurements above are why not. It is also why the loop never re-prints the art from scratch: with
`--reuse-canvas` the canvas is drawn once and each effect changes it in place, instead of a full
clear and repaint between effects.

### Not ported, on purpose

* **Deboma's launcher.** `hyprctl monitors`, a special workspace per monitor and a socket2 event
  stream for one output. The window rule fullscreens `[app_id="org.lazyos.screensaver"]` instead.
* **`ttfx` and its shim.** Omarchy's scripts call the Rust port by that name; this one calls Debian's
  `tte` directly, so there is nothing to shim and no second dependency.
* **A 120 fps option.** A faster loop would be a setting nobody can see the cost of; the loop is
  already smooth enough to read the wordmark.

### The two costs that are not CPU

* **Terminal traffic.** `tte` animates by writing ANSI to the pty, and foot repaints the cells that
  changed; a full repaint at 30 fps would be the thing to avoid, and `--reuse-canvas` is what keeps
  the writes to the changed cells.
* **The key that dismisses it.** `tte` reads the terminal for its own controls, so a key press while
  an effect runs went to `tte` (which ignores it) and not to the script's `read -t 1` poll: the screen
  survived the key until the effect ended. Starting `tte` with stdin on `/dev/null` gave the poll the
  only reader, and the dismissal is immediate.

## 11. The workspace renamer

The bar's pills have to say what is running where, and the only way to know that is to watch: sway
tells nobody which windows are in a workspace unless somebody keeps the IPC connection open.
`workstyle` (built here from its pinned crate, `tools/workstyle/`) opens one connection, blocks on
the event stream, and issues one `rename workspace to` when the set of windows in a workspace
changes. That is the whole loop, and it is the session's second resident process.

### Method

The shipped binary (`workstyle 0.8.2-1lazyos1`, `/usr/bin/workstyle`) started by the session with
the shipped config, one foot window in workspace 1 and Firefox in workspace 2. RSS and PSS are
`/proc/<pid>/status` and `/proc/<pid>/smaps_rollup` on the running daemon; CPU is `utime+stime` from
`/proc/<pid>/stat` sampled 60 s apart with nothing else running. The rename itself is what `swaymsg`
reports for the same command; it is a single IPC round trip.

### What it costs

| | Value |
|---|---|
| binary / package | 1.9 MB installed, 584 KB `.deb` |
| VmRSS | 2.4 MB (peak 4.7 MB, right after the first rename) |
| PSS | 1.0 MB (152 KB of it private-dirty; the rest is the mapped binary's shared text) |
| CPU while idle | 0 ticks in 60 s (`utime+stime` unchanged) |
| per workspace event | one `rename workspace to` IPC command, plus rewriting that workspace's name |

The rename is not free but it is bounded by the number of workspaces that *have* windows - in
practice one or two - and it happens on window open/close/move, not on every key press. The
alternative measured and rejected was waybar's own `{windows}` with `window-rewrite`: it needs no
process at all, and it was verified drawing a terminal glyph in the pill on this machine, but it
draws the windows *inside the bar's pill* and leaves the workspace name bare - `swaymsg -t
get_workspaces`, and therefore anything scripted, and any other bar, keeps seeing `3`. workstyle
renames the workspace itself, so every surface agrees about what is in workspace 3, and it is what
Nura ran.

### What it must not become

* **No polling.** The daemon waits on the socket; a 1 s `swaymsg -t get_tree` loop would wake the
  CPU 86 400 times a day and rewrite every name each time. The measured 0 ticks is the point.
* **No extra icons per window.** `deduplicate_icons = true` keeps a workspace of three terminals one
  glyph, which is both a shorter pill and less work per event.

## 12. zram sizing - item 11d, measured

The image shipped `zram-size = min(ram / 2, 4096)`, which is `systemd-zram-generator`'s own
default: 1.9 GiB on this 3.7 GiB machine. The cap decides **when the slow tier starts**, and the
next tier here is a microSD at ~2 MB/s of random 4 KiB writes, so the cap matters - but both
extremes are wrong for a different reason, and the value that ships is the middle (section below).

### What the device does (eMMC install, lz4, `/sys/block/zram0/mm_stat`)

| | |
|---|---|
| `orig_data_size` | 427 MiB stored |
| `compr_data_size` | 61.9 MiB - 6.9 : 1 on the bitstream |
| `mem_used_total` | 97.7 MiB - **4.4 : 1 in real RAM**, zsmalloc overhead included |
| `same_pages` | 55 432 of 109 360 - **half the pages are duplicates**, i.e. free |
| unique pages alone | 220 MiB -> 61.9 MiB -> 97.7 MiB - 3.56 : 1 bitstream, **2.25 : 1 resident** |
| `mem_used_max` | 312 MiB, against a 1 902 MiB cap |

Exchange rate: **0.23 GiB of RAM per GiB of fast tier** for the workload this machine runs, 0.44 GiB
if every page were unique. The captured load in section 1 sat at 1.4 GB of 1.948 GB (72 %) - closer
to the cap than a 4 GB machine should sit, though it never actually spilled, which matters below.

### What is written now, and why it is not `ram`

`rootfs-setup.sh` writes `zram-size = min(ram * 2 / 3, 4096)`, `compression-algorithm = lz4`,
`swap-priority = 100`: ~2.5 GiB on this machine, between the packaged 1.9 GiB and the full 3.7 GiB.

The full-RAM cap was shipped first (`zram-size = ram`) and reverted, because it introduced a small
but real latency the port had not had before. The cost of the *size* was not the problem - filling
3.7 GiB costs ~0.85 GiB of RAM on this workload, spent only as pages appear (peak so far: 312 MiB).
The problem was how the kernel behaves once zram never fills: `vm.swappiness=150` tells it to prefer
swapping over dropping clean page cache, and the full cap removes the backpressure that would
eventually force the free drop-cache path. With no cap to hit, the machine swaps out cold pages it
used to keep in RAM, and every re-touch faults one back through an lz4 decompress - the latency.
Two thirds of RAM keeps roughly double the old headroom while the cap still fills, so the kernel
falls back to dropping clean cache (free) instead of swapping indefinitely.

### Why `zram-resident-limit` is not used

It looks like the valve for incompressible data and it breaks the tier below it. At the limit
`zram_can_store_page()` refuses the store (`drivers/block/zram/zram_drv.c`) and `zram_bio_write`
turns that into `BLK_STS_IOERR`; the swap allocator chooses a device by priority using *slot
exhaustion*, not write success, so it keeps picking zram - whose nominal slots are free - and keeps
failing. The overflow becomes an I/O error on swapped-out data instead of a page handed to the card,
and the auxiliary swap this port ships becomes hibernate-only. Only the nominal cap falls through
cleanly (`si->inuse_pages == si->max` moves the allocator to the next priority).

### Why lz4 and not zstd

`CONFIG_ZRAM_MULTI_COMP` is not set, so the cheap form - `compression-algorithm = lzo-rle
zstd(level=3)`, which recompresses idle pages - parses and does nothing. Wholesale `zstd` buys
20-30 % ratio for 2-4x the CPU, charged on the A55 cluster at the moment the machine is already
starved, while doubling the cap buys 100 %. The A/B (`stress-ng --vm`) is still the thing to run
before changing the algorithm; enabling MULTI_COMP is a separate kernel-config decision.

### The shell as shipped (spec 002)

The design system was measured the same way, on this device, with the real shell rather than a
synthetic widget:

| what | release | unoptimised | allocations per frame |
|---|---|---|---|
| shell at 152x45, 40-row list, key bar | **424 µs** | 6.2 ms | **0** |
| empty draw (ratatui alone, §13 above) | 154 µs | - | - |

The extra 270 µs over the empty floor is cell copies (40 rows plus the 152-cell rule under the
header) and the backend's own diff - not formatting: a counting global allocator
(`tests/no_alloc.rs`, 100 frames after a warm-up) reports **zero** allocations, which is the
mechanical form of the "build the tree once per state" rule above. Two things were changed to get
there: `ListState` owns its rows and renders them into the buffer, and the shell computes its
three bands with arithmetic instead of `Layout::split`, which otherwise allocates a `Vec` of
rectangles every frame.

The tick is only paid while something changes: the loop blocks on `event::poll` with no timeout
when idle, and wakes at 10 fps only for the 300 ms opening sweep (tachyonfx). An idle window
therefore costs nothing, which is the same rule the session's "nothing resident" already applies
to processes.

The golden snapshots (`lazyos-ui/tests/snapshots/`, every widget at 80x24 and 152x45) are the
other half of the budget: the frame test says a tick is cheap, and the snapshots say what that
cheap frame actually looks like, so a change that moves a border is visible in review instead of
being inferred from a timing.

### How to check it took

`cat /sys/block/zram0/disksize` should be within 10 % of two thirds of `MemTotal` (the generator
rounds to whole MiB), and under the load that produced the 1.4 GB reading `orig_data_size` should
pass 1 902 MiB while the card's used column in `/proc/swaps` stays 0. `check-system.sh` asks both
halves - the file and the live device - and flags drift in *either* direction: a smaller device
spills to the card while RAM is free, and a larger one (a machine still booted on the old `ram`
device) carries the swap latency this section records.

## 13. What the UI framework costs - ratatui, and why not a ratatui-lite

The session UI is moving to Ratatui in foot ([`plan-ui.md`](plan-ui.md)). The question this
section answers is whether Ratatui is the expensive part: if it is, the alternative would be a
home-grown "ratatui-lite" with only the widgets the framework uses. Everything below is measured on
this device (aarch64, 4 GB, release builds, `rustc` 1.97.1), not estimated.

### Ratatui against crossterm alone

Two throwaway crates with the same job - draw a header, a 40-row list and a key bar, then exit - one
with `ratatui` 0.30.2 and one writing the cells with `crossterm` 0.29 directly. Each built into its
own empty target directory, then stripped.

| | crossterm only | ratatui | delta |
|---|---|---|---|
| clean `cargo build --release` | 15.1 s | 38.7 s | **+23.6 s** (once per clean build) |
| `target/` | 46 MB | 135 MB | +89 MB |
| stripped binary | 398 KB | 726 KB | **+328 KB** |
| start to first frame | 23 ms | 23 ms | 0 (both are the `script` floor) |
| PSS of the process | 233 kB | 233 kB | 0 - the buffer is in the noise |

So the compile cost is real but one-time, the binary cost is a third of a megabyte, and the runtime
floor is invisible. The product's memory is elsewhere: a text-only ratatui app inside foot is
**1.0 MB of PSS**, foot itself is **21.1-21.7 MB**, and the same app showing a 1366x912 picture plus
a 48 px icon is **13.3 MB** (decode + sixel encode buffers). For scale, the same session measured
`rofi -show drun` at **73 MB** that day cold (the 15.7-17.1 MB in section 8 is a warm run with the
icon cache populated) - the framework's budget is not the thing to worry about.

### What a frame costs (152x45, the panel's real grid)

`Terminal::draw` on a `TestBackend`, 5 000 frames each, release build:

| what the frame does | per frame | at 10 fps | at 30 fps |
|---|---|---|---|
| empty draw (buffer reset + diff of 6 840 cells) | **154 µs** | 0.15 % of a core | 0.46 % |
| one cell changes (spinner / clock) | 231 µs | 0.23 % | 0.7 % |
| 43 lines rebuilt with `format!` each frame | 445 µs | 0.45 % | 1.3 % |

The load-bearing line is the first one: **~0.15 % of one core is what ratatui itself charges**, and
it is the floor every frame pays whether it changes one cell or the whole screen. The rest is the
widget tree being rebuilt. That makes two rules for the framework:

- **Event-driven redraw, 10 fps only while something is moving** - which the plan already says. At
  that cadence the framework spends 0.2-0.4 % of a core, i.e. below the noise of one compositor
  frame.
- **Build the widget tree once per state, not once per frame.** The 445 → 231 µs difference is
  `format!` per row per frame; a list of 40 formatted rows is 40 allocations and 40 strings per
  tick to display text that did not change. Cache the `Vec<Line>`, mutate the selected index.

### Why not write a ratatui-lite

Stripping Ratatui saves at most the numbers above: **~0.15 % of a core at 10 fps, ~300 KB of binary
and ~24 s of a clean build**. Against that, a home-grown renderer owns exactly the code that is
cheap to write and expensive to get right: Unicode width (CJK, emoji, combining marks, and the
private-use Nerd Font glyphs this UI is made of), wide-cell and grapheme handling in the diff, line
wrapping, the constraint solver behind `Layout`, terminal capability quirks, and a snapshot backend
for tests. Every silent rendering bug this repository has already chased - a font without a
codepoint, a glyph that draws nothing, a menu row that loses its icon column - is that class of bug,
and none of them would be caught by a unit test on a diff algorithm.

Two cheaper levers were measured and rejected as well, so nobody re-runs them:

- **Pruning ratatui's features** (`default-features = false, features = ["crossterm",
  "layout-cache"]`): same build time (38.7 s), same binary (726 KB), `target/` 135 → 120 MB. The
  default feature set only gates the calendar widget, the `text!` macros and underline colour.
- **A harder release profile** (`lto = "thin"`, `codegen-units = 1`, `strip = "symbols"` on the
  ratatui crate): 726 → 660 KB (-9 %), frame cost unchanged (159 µs empty draw, within noise),
  build 38.5 → 51.8 s. Three hundred kilobytes and no runtime gain do not buy back the extra build
  time or the symbols a panic report needs.

### The list filter: a keystroke, not a frame

Filtering happens on a state change, so the per-frame rules above do not apply - but a keystroke
is still a person waiting, and the file list is thousands of rows. The first version lowercased
the needle *inside* the row test, so every row built a folded copy of the needle and three more
of its own fields:

| 5,000 rows, one keystroke (release) | before | after |
|---|---|---|
| worst key | **1.74 ms** | **0.37 ms** |
| a needle that matches the first field | 1.74 ms (no early exit: every field was rebuilt first) | **0.15 ms** |
| allocations per keystroke | ~20,000 (four per row) | **1-2**, independent of the row count |
| 200 rows instead of 5,000 | only the time changes | same allocations |

Three changes, each with a witness in `lazyos-ui/tests/filter.rs`:

- **The needle folds once per change**, never per row; the row test takes the folded form.
- **An ASCII needle takes a byte path** (`eq_ignore_ascii_case` over the bytes, with a single-byte
  fast case for the first key). UTF-8 never encodes an ASCII byte inside a multi-byte character,
  so folding `a-z`/`A-Z` *is* case-insensitivity for it, and nothing is allocated. An accented
  needle falls back to `to_lowercase` - the only form that folds `Ü` to `ü` - and pays one
  allocation per row only while it carries an accent.
- **Typing narrows instead of rebuilding**: a longer needle can only match a subset of a shorter
  one's matches (`"repo"` matches nothing `"rep"` did not), so the pass is a `retain` over the
  visible set - no scan of the full list, no allocation. A backspace rebuilds, which is why the
  table measures both, and `narrowing_gives_the_same_answer_as_a_fresh_filter` is the proof
  obligation that comes with the shortcut.

What is left is memory, not arithmetic: a row is three heap strings, so a needle that matches
almost nothing walks three allocations per row (~115 ns/row, ~3 cache misses). Packing the
searchable text into one lowercased buffer with an offset per row was measured too, and it is
faster on exactly that case - **151 µs against 443 µs** when few rows match, **420 against 543 µs**
when none do - but it loses on the early-match case (143 µs against 68 µs), costs **287 KB** for
5,000 rows, rebuilds the index on every `set_rows`, and keeps a second representation of every
row's text. That is 0.3 ms saved on a path already far below one frame, so the simple shape stays,
the same trade as the ratatui-lite above. The budget test pins it: 3 ms for the worst key in
release, 25 ms in debug (the debug number is the tripwire; `--release` is the device's).

### The dependency that actually costs: image codecs

The sixel spike pulled `image` 0.25 with its **default features**, which is every decoder and
encoder the crate has - AVIF via `ravif`, EXR, TIFF, WebP, GIF, QOI, PNM, TGA, DDS, HDR - to draw
two things: the theme's PNG icons and the pictures `lazyos-thumbnail` produces (PNG, and JPEG
straight from ffmpeg).

| `image` features | clean build | `target/` | stripped binary |
|---|---|---|---|
| defaults (all codecs) | 147 s | 272 MB | 2.17 MB |
| `png` + `jpeg` only | **49 s** | **49 MB** | **0.73 MB** |

On the spike itself that took the clean release build from **228 s to 115 s** and the binary from
8.6 MB unstripped to 2.21 MB (1.58 MB stripped). This is the one change from the whole evaluation
that is worth applying, and it is applied: `crates/spike-sixel/Cargo.toml` names `png` + `jpeg` and
drops ratatui-image's `image-defaults` (which would turn every codec back on). The framework
inherits the same rule: **a codec is added when a real file needs it, and with the numbers that say
why**, because each one is seconds of build and hundreds of kilobytes of binary shipped in a 4 GB
image.

### How to check it took

`cargo tree -p image --depth 1` lists only `png`/`zune-jpeg` among the decoders, and
`cargo build --release` from an empty target directory stays under two minutes on this machine. The
frame budget is checked by the design-system test (spec 002), which fails when a tick is above the
measured ceiling; the PSS numbers are re-taken with the method in section 8 whenever a new window
carries pictures.
