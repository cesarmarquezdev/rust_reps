# Rust Reps

Tarjetas de Rust con Elo, repesca y repetición espaciada. Tauri 2 + core Rust + SQLite local.
Tres modalidades: **predice** (elige), **completa** el código (huecos) y **ordena** las líneas, más el
**duelo contra la IA** (encuentra el bug) tras una ronda perfecta.

## Jugar

Descarga `RustReps.exe` de [Releases](https://github.com/cesarmarquezdev/rust_reps/releases/latest) y ábrelo:
es portable (Windows 10/11, sin instalar nada). Tu progreso queda en
`%LOCALAPPDATA%\local.rustreps.app\reps.sqlite3`.

## Compilar desde el código

El `.exe` no está en el repo; se compila así (Windows):

1. Instala [Rust](https://rustup.rs) y, cuando lo pida, las *Build Tools de Visual Studio* (C++).
2. WebView2 ya viene en Windows 10/11.
3. Clona y compila (la primera vez tarda varios minutos y baja ~6 GB de dependencias a `target/`):

```powershell
git clone https://github.com/cesarmarquezdev/rust_reps.git
cd rust_reps
cargo run -p rust-reps --release
```

El ejecutable queda en `target\release\rust-reps.exe`. Para correr los tests: `cargo test -p reps-core`.
No hace falta Node ni la CLI de Tauri: la UI (`ui/`) es HTML/JS estático.

## Libros (lector)

Botón **Libros**: lector con scroll continuo estilo Apple Books (temas, tamaño y tipo de letra, contenido, búsqueda, posición guardada). Los 8 libros (Rust, Python, Algoritmos, Datos y APIs, JavaScript y React, Testing, C++, Método y arquitectura) son archivos en `ui/books/*.js`, generados con `node _conocimiento/books.cjs` desde `_conocimiento/CONOCIMIENTO.md` (cada sección es un capítulo; los 728 conceptos están). Selecciona texto y toca **＋ Tarjeta**, o abre una pregunta y toca **Crear tarjeta**: se guarda en SQLite (`user_cards`) como tarjeta *voltear* ("lo sabía / no lo sabía"), sale primero en las rondas y usa el mismo Elo y repaso espaciado.

## Reglas

| | |
|---|---|
| Ronda | 5 tarjetas. Entrada −50 Elo (gratis si tienes < 50: entrenas y ganas +8 al completar) |
| Acierto / error / acierto en repesca | +20 / −12 / +8 (el Elo nunca baja de 0) |
| Repesca | La tarjeta fallada vuelve 2 tarjetas después hasta acertarla |
| Repaso espaciado | Primer intento correcto: 1 → 3 → 7 → 14 → 30 días. Fallo: vuelve hoy. Las vencidas entran primero en cada ronda |
| Rangos | Novato (0) · Aprendiz de Cargo (900) · Rustacean (1100) · Auditor de IA (1300) · Maestro del ownership (1500) |
| Racha diaria | Días seguidos con al menos una tarjeta acertada |

## Código

```text
core/src/content.rs   Contrato de las tarjetas y validación del mazo
core/src/engine.rs    Reglas: Elo, repesca, repaso espaciado, calificación, duelo (+ 9 tests)
core/src/store.rs     SQLite: perfil, card_state (caja/vencimiento), events (historial)
core/migrations/      Esquema versionado (PRAGMA user_version)
content/cards.json    46 tarjetas + 8 duelos (fuente de verdad del contenido)
desktop/src/main.rs   Adaptador Tauri: 7 comandos, sin lógica de juego
ui/                   Solo dibuja y manda respuestas con invoke()
```

El core no importa Tauri: la UI nunca recibe la solución antes de responder y no califica nada.
Eso permite reutilizarlo tal cual en el móvil.

## Añadir tarjetas

Edita `content/cards.json` (el mazo se valida al abrir y en `cargo test`). Cada tarjeta lleva
`id`, `topic`, `question`, `code`, `teaser` (gancho del botón "Siguiente"), `explanation` y según su `kind`:

- `choice`: `options` (3) y `answer` (índice). **Pon la correcta en una posición distinta cada vez**: un test falla si más de la mitad quedan en la misma letra.
- `fill`: `code` con `___` por hueco, `blanks` (texto correcto de cada hueco) y `bank` (fichas, con distractores y sin empezar por la solución).
- `order`: `lines` mezcladas y `solution` (índices en el orden correcto).

Un duelo se elige por el `topic` de la tarjeta que fallaste, así que su `topic` debe existir en alguna tarjeta.
Antes de publicar una tarjeta, comprueba cada afirmación con `rustc`.

## Comandos

```powershell
cargo test -p reps-core --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo build -p rust-reps --release --offline
```

`--offline` solo funciona si ya tienes las dependencias descargadas; en un clon nuevo quítalo.
Para probar sin tocar tu progreso: `REPS_DATA_DIR=<carpeta>`.

## Límites conocidos

- Un solo perfil y sin sincronización. El historial `events` está pensado para sincronizar con el móvil.
- El icono es el de `ascension` (placeholder).
- Los intervalos 1·3·7·14·30 son prácticos, no un calendario óptimo demostrado.
- Móvil: `dist/RustReps-android.apk` (debug, ARM64). Compilar en Windows sin Modo desarrollador falla al crear enlaces simbólicos: copiar `librust_reps_lib.so` a `desktop/gen/android/app/src/main/jniLibs/arm64-v8a/` y `gradlew assembleArm64Debug -x rustBuildArm64Debug`.
- Las tarjetas propias no se pueden editar ni borrar todavía; el lector solo guarda preferencias y posición en `localStorage`.
- Abandonar una ronda a medias (cerrar la app) no devuelve la entrada de 50 Elo.
