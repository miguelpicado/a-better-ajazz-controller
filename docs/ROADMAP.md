# ROADMAP (2026-11-10) — qué falta para dar la app por terminada y qué se investiga después

Objetivo rector en `docs/HANDOFF.md` ("ultraligero, todo Rust, UI web bajo demanda"). Estado de código y hallazgos: `HANDOFF.md`/`STATUS.md`.

## A. App "100 %" sin pulsación larga

| # | Pieza | Estado | Dueño / rama |
|---|---|---|---|
| 1 | `run`: driver + host + builtin, lotes, reconexión | hecho, probado en vivo | main |
| 2 | Títulos, estados, ok/alert | hecho (mergeado `cacec39`), sin prueba física | main |
| 3 | Límites zip + overlay obsoleto en `plugman` | hecho (`bef44ad`) | main |
| 4 | `orcadeck edit` (editor web bajo demanda) | hecho (`08bf0d9`+`846bcdf`; 0 s CPU y 7 MB RSS en reposo, medido) | main |
| 5 | `Scene Shift` | diseño hecho (`docs/research/scene-shift.md`); implementación por lanzar (`miguelpicado/scene-shift-impl`) | worker |
| 6 | `Settings`/`GlobalSettings` persistidos, `OpenUrl`, reinicio de plugins con backoff, feedback sin hilo | hecho (`c1e22f3`; exe 3 422 720 B tras merge de todo) | main |
| 7 | Inspector de propiedades (PI) desde el editor: el editor vive en otro proceso que el host → decidir: `edit` dentro del daemon en marcha (puerto 18618 ya es del host) o proceso aparte con IPC mínimo | sin diseñar | coordinador |
| 8 | Instalar/quitar plugins desde la UI con `plugman` + vigilar `Downloads` | sin empezar | worker tras 4 |
| 9 | Autoarranque (`tools/autostart/orcadeck-task.ps1`) | tarea `OrcaDeck` instalada DESACTIVADA; activar exige consola elevada: `schtasks /change /tn StreamDock /disable` y luego `.\orcadeck-task.ps1 enable` | usuario + mañana |
| 10 | Medidas de ligereza: tamaño exe (3 219 456 B hoy), RSS y CPU en reposo con el dispositivo, `ORCADECK_TRACE=1` | pendiente (necesita dispositivo) | mañana |
| 11 | Hilo por feedback (600 ms) → un temporizador único en el hilo principal | optimización menor | worker |

## B. Pruebas físicas de mañana (monitores encendidos)
1. Parar Stream Dock (`schtasks /end /tn StreamDock`), `orcadeck run` perfil Orca: títulos, estados, ok/alert, gif, pulsaciones (tras fusionar ramas: recompilar).
2. `orcadeck edit`: editar una escena, comprobar que `run` la recoge.
3. Medir RSS/CPU/tamaño y anotarlos aquí.
4. Activar el autoarranque (con consola elevada) y reiniciar sesión.

## C. Siguiente investigación: pulsación larga / pulsar-soltar con firmware (NO flashear sin orden explícita)
Contexto: `docs/research/firmware/patch-emulation.md` (v2 ≈ 80 % correcto, sin red de recuperación). Orden seguro:
1. **Ensayo de reflasheo idéntico del original**: `orcadeck hidwatch` corriendo en otra consola para ver `EEEF:2929`; luego la herramienta oficial con el `.bin` original (`backup/AKP153E.bin`, CRC `0xC065`). OJO: `FirmwareUpgradeTool.exe` con 4 argumentos flashea al instante (`768 4112 <bin> <icono>`). Demuestra que el bootloader entra en modo actualización y que se vuelve al original.
2. Buscar vía de recuperación de hardware: identificar el chip (C*Core), ver si la placa expone SWD o un pin BOOT (abrir la unidad; fotos).
3. Con (1) y (2) hechos: flashear v2 y probar `watch` (debe salir `state=1` al pulsar y `0` al soltar).
4. Cliente: `orcadeck` ignora `state=1` repetido de una tecla ya pulsada (ACK del ISR); pulsación larga con temporizador de 550 ms.
5. Alternativa sin riesgo: unidad AKP153E rev. 2 (PID 0x3010).
