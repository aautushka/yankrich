#!/usr/bin/env bash
# Print a variety of styled output for testing yankrich.

ESC=$(printf '\033')
R="${ESC}[0m"

printf "%s\n" "--- 16 foreground colors ---"
for i in 30 31 32 33 34 35 36 37 90 91 92 93 94 95 96 97; do
    printf "${ESC}[%sm fg%s ${R}" "$i" "$i"
done
printf "\n\n"

printf "%s\n" "--- 16 background colors ---"
for i in 40 41 42 43 44 45 46 47 100 101 102 103 104 105 106 107; do
    printf "${ESC}[%s;30m bg%s ${R}" "$i" "$i"
done
printf "\n\n"

printf "%s\n" "--- bold / italic / underline ---"
printf "${ESC}[1mbold${R}  ${ESC}[3mitalic${R}  ${ESC}[4munderline${R}  ${ESC}[1;4;31mbold-underline-red${R}\n\n"

printf "%s\n" "--- 256-color palette samples ---"
for i in 196 202 208 214 220 226 154 118 82 46 51 45 39 33 27 21 93 129 165 201; do
    printf "${ESC}[38;5;%sm#%3d${R} " "$i" "$i"
done
printf "\n\n"

printf "%s\n" "--- truecolor gradient ---"
for x in $(seq 0 7); do
    r=$((255 - x * 32))
    g=$((x * 32))
    b=128
    printf "${ESC}[48;2;%d;%d;%dm   ${R}" "$r" "$g" "$b"
done
printf "\n\n"

printf "%s\n" "--- mixed combos ---"
printf "${ESC}[1;33;44m bold yellow on blue ${R}  ${ESC}[3;37;41m italic white on red ${R}  ${ESC}[4;32m underline green ${R}\n"
