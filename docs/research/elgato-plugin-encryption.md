# Cifrado de manifests de plugins de Elgato Marketplace

Registro de lo observado el 2026-11-09. Resumen de `manifest-profiles-admin.md` §1, con las pruebas y los límites.
Etiquetas: OBSERVED (visto en ficheros, strings o decompilación), INFERRED (deducido, no comprobado).

## Qué es

- Los plugins distribuidos por el Marketplace de Elgato llevan el `manifest.json` cifrado. Es una protección de
  distribución (DRM) de Elgato, no un detalle del formato de plugin. OBSERVED.
- Los plugins que Elgato incluye con la app (`Distributables\*.streamDeckPlugin`: keycreator, tutorial) y los de
  distribución abierta llevan manifest en JSON plano. OBSERVED.
- Plugins cifrados encontrados en esta máquina: com.barraider.advancedlauncher, com.elgato.camerahub,
  com.elgato.obsstudio, com.elgato.volume-controller, com.insta360.webcam, com.vivremotion.windowscockpit. OBSERVED.

## Qué está cifrado y qué no

- Solo el manifest. Código, imágenes, ficheros de idioma (`en.json`), HTML y ejecutables están en claro dentro de la
  carpeta del plugin. OBSERVED.
- Consecuencia: copiar la carpeta no requiere descifrar nada. Lo que no se puede leer es la lista de acciones,
  `CodePath`, `Controllers`, etc., que viven en el manifest cifrado.
- En la carpeta del plugin no hay clave, certificado ni firma. OBSERVED.

## Formato del fichero (OBSERVED el layout, INFERRED el significado)

```
off  size  valor
0    6     "ELGATO"                 magic
6    2     u16 LE = 1               versión del formato
8    8     u64 LE = tamaño total    coincide con el tamaño del fichero
16   8     u64 LE = N               múltiplo de 32 (160, 448, 320, 640, 256, 192)
24   N     bloque de alta entropía  INFERRED: cabecera/sobre de clave (clave pública efímera, nonce, firma…)
24+N resto cuerpo cifrado           entropía 7.98–8.00 bits/byte
```

El tamaño crece con el número de ficheros del plugin (136–200 bytes por fichero). INFERRED: el payload contiene el
manifest más una lista de checksums de todo el plugin.

## Esquema criptográfico (STRINGS en `StreamDeck.exe` de Elgato)

- Esquema `x25519-hkdf-sha256-aes-256-gcm`: acuerdo de clave X25519, HKDF-SHA256, AES-256-GCM.
- Módulo interno `esd::crypto::scheme::{Decryptor, Decryptor1, Signature, Signature_Ed25519, ArchiveReader…}` y
  hash Blake3. OpenSSL 3 enlazado estáticamente.
- Firma Ed25519 y verificación de checksums por fichero (`esd::plugins::verify_checksum`, `hash mismatch`).
- Mensajes de comprobación específicos del Marketplace: `Marketplace plugin is missing identity`, `missing codesign`,
  `invalid SDK version`. Los plugins del Marketplace pueden llevar además `secrets.json` cifrado.
- Los datos descifrados se guardan cifrados en RAM (`esd::crypto::memory_vault`).

## Quién descifra

- La app Stream Deck de Elgato, al cargar el plugin. No el instalador ni el plugin. OBSERVED.
- No existe copia en claro del manifest en el disco (ni en carpetas de plugin, `Cache.db`, `Marketplace\cache.json`,
  `Data\application_cache.json` ni `%LOCALAPPDATA%\Elgato`). OBSERVED.
- `Marketplace\user.json` contiene la cuenta firmada e `cache.json` una lista de derechos firmada. INFERRED: el
  descifrado está ligado a una instalación con cuenta de Elgato con derecho al plugin.
- Al ser X25519 asimétrico, el fichero va cifrado hacia una clave pública y la clave privada la tiene el lado de
  Elgato, ya sea embebida en la app o derivada de la cuenta. No es observable desde los ficheros del plugin. No se
  buscó ni se extrajo (hacerlo sería saltarse la protección).

## Qué hace Stream Dock (AJAZZ)

- Su cargador lee `manifest.json` con `QJsonDocument::fromJson` (DECOMPILED). Un fichero `ELGATO` da un objeto vacío
  y el plugin se descarta. No incluye ningún descifrador (no hay literal `ELGATO`, `x25519`, `hkdf` ni
  `aes-256-gcm`; solo importa OpenSSL 1.0, que no tiene X25519). OBSERVED/INFERRED.
- Por eso Insta360 no funcionó hasta sustituir su manifest por uno en claro (`tools/streamdock/insta360-manifest.json`;
  el original quedó como `manifest.json.elgato`).
- Dato sin explicar: los logs muestran 5 plugins cifrados conectados hasta las 15:31 del 2026-11-09. Hipótesis sin
  verificar: procesos de plugin que sobrevivieron a un reinicio de Stream Dock y se reconectaron.

## Cómo funcionan los plugins sin manifest legible

- El SDK `@elgato/streamdeck` de Elgato que lleva el propio plugin prevé este caso: `getManifest()` devuelve `null`
  cuando el plugin está protegido por DRM (OBSERVED en `bin/plugin.js` de Insta360). El plugin en ejecución se
  comporta igual con el manifest en claro o sin él.
- El manifest solo lo necesita el host: `CodePath`, versión de Node, lista de acciones, `Controllers`, `States`,
  iconos.

## Decisión para la app nueva

| Vía | Estado |
|---|---|
| Descifrar el manifest (clave de Elgato o usar su app como oráculo) | Descartada: es saltarse el DRM. |
| Manifest en claro publicado por el proveedor (build abierta, GitHub, tienda de Stream Dock) | Posible solo si el proveedor lo publica. |
| Manifest reconstruido desde los ficheros en claro del plugin (`en.json`, código, estructura de carpetas) | Recomendada. Receta en `manifest-profiles-admin.md` §1.6. |

## Pregunta abierta (investigación del usuario)

Instalar un plugin de Elgato sin la app Stream Deck exige obtener su paquete (`.streamDeckPlugin`, un zip) por una
vía legítima. Para plugins con manifest en claro basta descomprimirlo en la carpeta de plugins. Para los del
Marketplace el paquete trae el manifest cifrado, así que además hace falta un manifest reconstruido o publicado por el
proveedor. Cómo obtener el paquete del Marketplace sin la app de Elgato no está investigado.
