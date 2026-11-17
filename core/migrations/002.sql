BEGIN;
-- Tarjetas que crea el usuario desde el lector (frente/dorso, se califican "lo sabía / no lo sabía").
CREATE TABLE user_cards (
    id TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    front TEXT NOT NULL,
    back TEXT NOT NULL,
    created TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
PRAGMA user_version = 2;
COMMIT;
