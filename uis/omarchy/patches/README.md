# patches/ — changes to the vendored Omarchy tree

The vendored tree under `files/usr/share/omarchy/` is upstream's, byte for byte, except
where a patch here says otherwise. Nothing is edited in place: a change is a `*.patch`
against the pinned commit plus a line in `series`, applied in that order — the same
convention as `kernels/<flavor>/patches/`.

`series` is empty for now: the port has not needed to patch upstream yet. The Arch→Debian
adaptation lives in `files/` outside the vendored tree (the package list, the session
entry, the device quirks), not inside it. When a vendored script does need to change, add
its patch here and reference it from `README.md`.
