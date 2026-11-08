{{ header }}
# The lock screen's palette. The photograph stays what it is (the desktop is the
# surface the brand owns; the lock is the one picture that is not the mark), so
# the colours only tint the indicator and the failed-attempt ring.
color={{ background:bare }}
inside-color={{ background:bare }}aa
inside-clear-color={{ background:bare }}bb
inside-caps-lock-color={{ background:bare }}bb
inside-ver-color={{ background:bare }}bb
inside-wrong-color={{ background:bare }}bb
ring-color={{ accent_alt:bare }}
ring-clear-color={{ ok:bare }}
ring-caps-lock-color={{ warn:bare }}
ring-ver-color={{ accent:bare }}
ring-wrong-color={{ danger:bare }}
key-hl-color={{ strong:bare }}
bs-hl-color={{ danger:bare }}
separator-color={{ surface_alt:bare }}
text-color={{ text:bare }}
text-clear-color={{ text:bare }}
text-caps-lock-color={{ text:bare }}
text-ver-color={{ text:bare }}
text-wrong-color={{ text:bare }}
line-color={{ background:bare }}00
line-clear-color={{ background:bare }}00
line-caps-lock-color={{ background:bare }}00
line-ver-color={{ background:bare }}00
line-wrong-color={{ background:bare }}00
image={{ home }}/Pictures/Wallpapers/{{ wallpaper }}
scaling=fill
indicator-radius=140
indicator-thickness=6
font=JetBrains Mono
font-size=18
show-failed-attempts
