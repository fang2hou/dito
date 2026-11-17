#!/bin/bash
W="${FILM_WORK:?set FILM_WORK}"; B="${FILM_BIN:?set FILM_BIN to the capture build of dito}"
L=$1
cd $W/real
rm -rf d_$L dh_$L dt_$L intro_$L hud_$L tools_$L
DITO_SHOTS_ONLY=introfilm DITO_FILM_LANG=$L timeout 300 $B --mock --log-sink --data-dir $W/real/d_$L --screenshots $W/real/intro_$L > intro_$L.log 2>&1; echo intro $L exit=$?
DITO_FILM_HUD_ONLY=1 DITO_SHOTS_ONLY=readmefilm DITO_FILM_LANG=$L DITO_FILM_THEME=dark DITO_FILM_SCENES=dictation,command timeout 300 $B --mock --log-sink --data-dir $W/real/dh_$L --screenshots $W/real/hud_$L > hud_$L.log 2>&1; echo hud $L exit=$?
DITO_SHOTS_ONLY=readmefilm DITO_FILM_LANG=$L DITO_FILM_THEME=light DITO_FILM_SCENES=meeting,chat timeout 400 $B --mock --log-sink --data-dir $W/real/dt_$L --screenshots $W/real/tools_$L > tools_$L.log 2>&1; echo tools $L exit=$?
echo ALLDONE $L
