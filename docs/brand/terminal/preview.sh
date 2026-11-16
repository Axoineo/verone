#!/usr/bin/env bash
# Preview the Verone terminal logos in brand colours (needs a truecolor terminal).
# Prints plain text when NO_COLOR is set or stdout is not a terminal.
dir=$(dirname "$0")
if [[ -t 1 && -z ${NO_COLOR:-} ]]; then g=$'\e[38;2;164;198;57m'; b=$'\e[1m'; d=$'\e[2m'; r=$'\e[0m'; else g= b= d= r=; fi
show() { # file, column width, info lines...
  local file=$1 width=$2; shift 2; local info=("$@") i=0 line
  while IFS= read -r line; do printf '%s%-*s%s  %s\n' "$g" "$width" "$line" "$r" "${info[i]}"; ((i++)); done < "$file"; echo
}
show "$dir/logo-large.txt" 23 "" "" "" "${b}${g}verone${r} 0.1.0" "${d}a virtual Android phone${r}" "${d}on your PC, laptop or server${r}"
show "$dir/logo-small.txt" 13 "" "${b}${g}verone${r} 0.1.0"
g= b= d= r=; show "$dir/logo-ascii.txt" 23 "" "" "" "verone 0.1.0"
