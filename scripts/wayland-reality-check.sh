#!/bin/sh
set -eu

if [ -z "${WAYLAND_DISPLAY:-}" ]; then
  printf '%s\n' '{"schema":1,"completed":false,"reason":"not_a_wayland_session"}'
  exit 2
fi

desktop=$(printf '%s' "${XDG_CURRENT_DESKTOP:-unknown}" | tr '[:upper:]' '[:lower:]')
case "$desktop" in
  *gnome*) compositor=gnome ;;
  *kde*|*plasma*) compositor=kde ;;
  *sway*) compositor=sway ;;
  *) compositor=other ;;
esac

registry=""
if command -v wayland-info >/dev/null 2>&1; then
  registry=$(wayland-info 2>/dev/null || true)
fi

data_control=false
case "$registry" in
  *ext_data_control_manager_v1*|*zwlr_data_control_manager_v1*) data_control=true ;;
esac

global_shortcut=false
if command -v busctl >/dev/null 2>&1 \
  && busctl --user introspect org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop \
    org.freedesktop.portal.GlobalShortcuts >/dev/null 2>&1; then
  global_shortcut=true
fi

paste_injection=false
if command -v wtype >/dev/null 2>&1 || command -v ydotool >/dev/null 2>&1; then
  paste_injection=true
fi

if [ "$data_control" = true ] && [ "$global_shortcut" = true ] && [ "$paste_injection" = true ]; then
  decision=full
elif [ "$global_shortcut" = true ]; then
  decision=capture_on_summon
else
  decision=unsupported
fi

printf '{"schema":1,"completed":true,"compositor":"%s","data_control":%s,"global_shortcut":%s,"paste_injection":%s,"decision":"%s"}\n' \
  "$compositor" "$data_control" "$global_shortcut" "$paste_injection" "$decision"
