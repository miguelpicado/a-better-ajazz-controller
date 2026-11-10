# Búsqueda de builds antiguas de Stream Dock AJAZZ y herramienta de firmware (2026-11-10)

Alcance: solo localización. No se descargó, ejecutó ni instaló ningún binario. Solo se leyeron páginas HTML con WebFetch/WebSearch. No se hicieron peticiones HEAD/GET a rutas de key123.vip (ninguna ruta de manifiesto .json conocida; no se hizo fuerza bruta). Contenido web = dato no confiable.

Limitaciones: web.archive.org está bloqueado en la herramienta de fetch (no se pudo consultar Wayback/CDX). manuals.plus devuelve 403. La página de versiones de support.key123.vip carga la tabla dinámicamente (vacía en el fetch).

## Hallazgos

### 1. La versión 2.9.177.122 SÍ traía una herramienta de firmware (confianza: media-alta)
- Fuente: https://advanceduninstaller.com/Stream-Dock-AJAZZ-Global-14c687450be4cf6829ae81599296b581-application.htm (artículo del 2024-06-28; es una guía de desinstalación, no un host de instalador).
- Stream Dock AJAZZ Global 2.9.177.122, editor HotSpot, ruta `C:\Program Files (x86)\Stream Dock AJAZZ Global`, MSI (`MsiExec /X{8A8FC22A-75DE-452D-91FA-AD1F750420AC}`).
- Ejecutables listados: `Stream Dock AJAZZ.exe` 19.989.112 bytes, **`FirmwareUpgradeTool.exe` (~1,62 MB)**, SystemMonitor.exe, QtWebEngineProcess.exe, ScreenCaptureTool.exe, SplashScreen.exe. Total ~24.651.848 bytes.
- Implicación: en esa época el actualizador era `FirmwareUpgradeTool.exe` dentro de la app, no `UpDateToolV2\UpDateTool11-11.exe`. La herramienta de feb-2024 (`UpDateToolV2`) probablemente se descargaba aparte o venía en una build 2.8.x/2.9.x intermedia. No se halló confirmación pública.
- La misma página lista otras versiones conocidas del producto: 3.10.197.1210, 2.10.186.1213, 3.10.191.0421, 3.10.200.0420, 2.10.179.426, 3.10.195.0902. Además existe 2.9.178.219 (otra página de advanceduninstaller, vista solo en el título de búsqueda). 2.9.174 y 3.10.191 aparecen sin instalador alojado.

### 2. Instaladores antiguos: no hay URL pública de descarga (confianza: media)
- No se encontró ningún mirror, Wayback, GitHub o foro con instaladores 2.8.x/2.9.x/2.10.x previas a feb-2024 ni 3.10.x antiguas. advanceduninstaller solo confirma cadenas de versión. Sin URL, fecha de publicación del instalador, tamaño ni hash.
- Las rutas actuales de Mirabox (https://mirabox.net/pages/download, leído hoy) son: Windows `https://key123.vip/win` (MSI, Stream Dock 3.10.203), Mac `https://key123.vip/mac` (3.10.185.1120), betas en `https://download.miraboxeco.com/mirabox/...` (p. ej. `streamdock-Install_Window_Beta.exe`, `Stream-Dock-Installer_Windows_Mira75.msi`, `Stream-Dock-Installer_Windows_Game.msi` 3.10.195.1021). Sin fechas, tamaños ni hashes publicados. Son builds de la marca Mirabox, no AJAZZ.
- Historial oficial de versiones: https://support.key123.vip/faqs/streamVersion.html (y /zh/faqs/streamVersion.html). La tabla (Name, Version, Release Date, Package, Log File) no se renderizó en el fetch; probablemente se carga por JS. Es la mejor pista para URLs de paquetes antiguos: abrirla en un navegador y mirar la columna Package/red. Confianza alta de que existe, no verificado el contenido.

### 3. Patrones de CDN key123.vip observados (confianza: media-alta, solo URLs publicadas en páginas oficiales)
- `https://cdn1.key123.vip/VSDCraft/VSD%20M18UpDatetool.zip` (herramienta de upgrade del VSD M18, hermano OEM de la misma plataforma). Fuente: https://www.vsdinside.com/blogs/tutorial/rgb-upgrade-vsd-m18. Es el único precedente público de un paquete `*UpDatetool*` en `cdn1.key123.vip`: patrón `cdn1.key123.vip/<NombreApp>/<Nombre>UpDatetool.zip`, con espacios codificados.
- `https://cdn1.key123.vip/mirabox/content/2026/09/<uuid>.pdf` (manual; patrón de contenido con año/mes y uuid, no adivinable).
- `https://key123.vip/win`, `/mac`, `/short` (redirectores de descarga), `https://support.key123.vip/...`, `https://space.key123.vip/StreamDock/plugins?software=StreamDock&client=VSDCraft` (plugins), `https://bbs.key123.vip` (foro).
- Hipótesis (no verificada, no probada): por analogía, una herramienta AJAZZ podría estar en `cdn1.key123.vip/<carpeta-AJAZZ>/...UpDateTool...zip`. Se desaconseja probar rutas a ciegas. Mejor: capturar el tráfico de red del propio cliente que actualice o inspeccionar su config/strings local (carpeta de la app, `AppData\Roaming\HotSpot\...`) para el nombre exacto.
- No se halló ningún manifiesto .json público ni mención de `UpDateToolV2`, `UpDateTool11-11`, `StreamDockUpDate` en ningún buscador. Búsquedas exactas: 0 resultados.
- Un manual de AKP153 (vía resumen de búsqueda, manuals.plus) cita un dominio de descarga `AJAZZ.keyl23.vip` (con "l", no "1"): sospechoso de typosquat o error tipográfico del manual; no abrir sin verificar.

### 4. Páginas oficiales de firmware para AKP153E (confianza: media)
- No existe una página dedicada de "firmware update" para AKP153E/293S/293SV3 encontrada. Los manuales (manuals.plus, vía resumen) solo dicen "descarga el software Stream Dock AJAZZ del sitio oficial" y "revisa la web por actualizaciones".
- Sitio oficial AJAZZ: https://www.a-jazz.com. Páginas de drivers: cliente https://www.a-jazz.com/h-col-155.html, web https://www.a-jazz.com/h-col-156.html, HUB web https://www.a-jazz.com/h-col-157.html, manuales https://www.a-jazz.com/h-col-102.html, feedback https://www.a-jazz.com/h-col-171.html. La página 1 de 18 de h-col-155 solo lista drivers de teclados (ALUX68, AK980, MK87; enlaces en download.s21i.co99.net). Las páginas 2-18 no se revisaron: ahí podría estar Stream Dock/"desktop console". Pendiente.
- ajazzstore.com/pages/download: 404.
- Mirabox (marca hermana): sin sección de firmware; solo software. Dev Log#08 (https://mirabox.net/blogs/news/...dev-log-08...) no menciona firmware ni key123.
- VSD (OEM de la misma plataforma): https://www.vsdinside.com/pages/download hoy no lista tool de firmware (la de M18 solo está en el blog tutorial). Los Craft actuales son 3.10.205.

## Siguientes pasos sugeridos (sin riesgo)
1. Abrir en navegador https://support.key123.vip/faqs/streamVersion.html y leer URLs de la columna Package (instaladores antiguos con fecha).
2. Revisar páginas 2-18 de https://www.a-jazz.com/h-col-155.html (y 156/157) buscando AKP153E/Stream Dock.
3. Wayback manual (bloqueado desde aquí): https://web.archive.org/web/2024*/https://www.a-jazz.com/* y `key123.vip/win`.
4. Buscar en el equipo (solo lectura) el binario de la app 2.9.177.122 (`FirmwareUpgradeTool.exe`) y strings con `cdn1.key123.vip` en las carpetas de versiones antiguas ya presentes.
5. Contactar soporte: foro https://bbs.key123.vip o support de AJAZZ pidiendo el paquete de firmware del AKP153E.
