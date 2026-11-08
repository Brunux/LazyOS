{{ header }}
# The lock screen's palette. The photograph stays what it is (the desktop is the
# surface the brand owns; the lock is the one picture that is not the mark), so
# the colours only tint the indicator and the failed-attempt ring.
color={{ background }}ee
inside-color={{ background }}aa
inside-clear-color={{ background }}bb
inside-caps-lock-color={{ background }}bb
inside-ver-color={{ background }}bb
inside-wrong-color={{ background }}bb
ring-color={{ accent_alt }}
ring-clear-color={{ ok }}
ring-caps-lock-color={{ warn }}
ring-ver-color={{ accent }}
ring-wrong-color={{ danger }}
key-hl-color={{ strong }}
bs-hl-color={{ danger }}
separator-color={{ surface_alt }}
text-color={{ text }}
text-clear-color={{ text }}
text-caps-lock-color={{ text }}
text-ver-color={{ text }}
text-wrong-color={{ text }}
line-color={{ background }}00
line-clear-color={{ background }}00
line-caps-lock-color={{ background }}00
line-ver-color={{ background }}00
line-wrong-color={{ background }}00
image={{ home }}/Pictures/Wallpapers/{{ wallpaper }}
scaling=fill
indicator-radius=140
indicator-thickness=6
font="JetBrains Mono"
font-size=18
show-failed-attempts
