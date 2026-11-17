BEGIN;
CREATE TABLE profile (
    id INTEGER PRIMARY KEY CHECK(id=1),
    elo INTEGER NOT NULL,
    best_secs INTEGER,
    streak INTEGER NOT NULL DEFAULT 0,
    last_day TEXT,
    rounds INTEGER NOT NULL DEFAULT 0
);
INSERT INTO profile(id, elo) VALUES (1, 1000);
-- Repetición espaciada: una fila por tarjeta ya vista.
CREATE TABLE card_state (
    card_id TEXT PRIMARY KEY,
    box INTEGER NOT NULL CHECK(box BETWEEN 0 AND 5),
    due TEXT NOT NULL,
    hits INTEGER NOT NULL DEFAULT 0,
    misses INTEGER NOT NULL DEFAULT 0
);
-- Historial: base para estadísticas futuras y para sincronizar con el móvil.
CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    card_id TEXT NOT NULL,
    correct INTEGER NOT NULL,
    retry INTEGER NOT NULL,
    delta INTEGER NOT NULL
);
PRAGMA user_version = 1;
COMMIT;
