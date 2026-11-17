//! SQLite: perfil, estado de repetición espaciada por tarjeta e historial.
use rusqlite::{Connection, params};
use std::{collections::HashMap, path::Path};

pub type Res<T> = Result<T, String>;
fn e(err: rusqlite::Error) -> String {
    format!("SQLite: {err}")
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub elo: i64,
    pub best_secs: Option<i64>,
    pub streak: i64,
    pub last_day: Option<String>,
    pub rounds: i64,
}

#[derive(Clone, Debug)]
pub struct CardState {
    pub box_: i64,
    pub due: String,
    pub hits: i64,
    pub misses: i64,
}

pub struct Event<'a> {
    pub card_id: &'a str,
    pub correct: bool,
    pub retry: bool,
    pub delta: i64,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Res<Self> {
        Self::init(Connection::open(path).map_err(e)?)
    }

    pub fn memory() -> Res<Self> {
        Self::init(Connection::open_in_memory().map_err(e)?)
    }

    fn init(conn: Connection) -> Res<Self> {
        conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get::<_, String>(0))
            .map_err(e)?;
        conn.execute_batch("PRAGMA synchronous=FULL; PRAGMA busy_timeout=5000;")
            .map_err(e)?;
        let check: String = conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(e)?;
        if check != "ok" {
            return Err(format!("La base de datos está dañada: {check}"));
        }
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(e)?;
        match version {
            0 => {
                conn.execute_batch(include_str!("../migrations/001.sql"))
                    .map_err(e)?;
                conn.execute_batch(include_str!("../migrations/002.sql"))
                    .map_err(e)?;
            }
            1 => conn
                .execute_batch(include_str!("../migrations/002.sql"))
                .map_err(e)?,
            2 => {}
            // No se baja de versión: mejor un error visible que perder datos.
            _ => return Err("La base es de una versión más nueva de la app".into()),
        }
        Ok(Self { conn })
    }

    pub fn profile(&self) -> Res<Profile> {
        self.conn
            .query_row(
                "SELECT elo,best_secs,streak,last_day,rounds FROM profile WHERE id=1",
                [],
                |r| {
                    Ok(Profile {
                        elo: r.get(0)?,
                        best_secs: r.get(1)?,
                        streak: r.get(2)?,
                        last_day: r.get(3)?,
                        rounds: r.get(4)?,
                    })
                },
            )
            .map_err(e)
    }

    pub fn add_user_card(&self, id: &str, topic: &str, front: &str, back: &str) -> Res<()> {
        self.conn
            .execute(
                "INSERT INTO user_cards(id,topic,front,back) VALUES(?1,?2,?3,?4)",
                params![id, topic, front, back],
            )
            .map(|_| ())
            .map_err(e)
    }

    /// (id, topic, front, back) en orden de creación.
    pub fn user_cards(&self) -> Res<Vec<(String, String, String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id,topic,front,back FROM user_cards ORDER BY created,rowid")
            .map_err(e)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map_err(e)?;
        rows.collect::<Result<_, _>>().map_err(e)
    }

    pub fn cards(&self) -> Res<HashMap<String, CardState>> {
        let mut stmt = self
            .conn
            .prepare("SELECT card_id,box,due,hits,misses FROM card_state")
            .map_err(e)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    CardState {
                        box_: r.get(1)?,
                        due: r.get(2)?,
                        hits: r.get(3)?,
                        misses: r.get(4)?,
                    },
                ))
            })
            .map_err(e)?;
        rows.collect::<Result<_, _>>().map_err(e)
    }

    /// Guarda perfil, estado de la tarjeta y evento en una sola transacción.
    pub fn record(
        &mut self,
        p: &Profile,
        card: Option<(&str, &CardState)>,
        event: Option<Event>,
    ) -> Res<()> {
        let tx = self.conn.transaction().map_err(e)?;
        tx.execute(
            "UPDATE profile SET elo=?1,best_secs=?2,streak=?3,last_day=?4,rounds=?5 WHERE id=1",
            params![p.elo, p.best_secs, p.streak, p.last_day, p.rounds],
        )
        .map_err(e)?;
        if let Some((id, s)) = card {
            tx.execute(
                "INSERT INTO card_state(card_id,box,due,hits,misses) VALUES(?1,?2,?3,?4,?5)
                 ON CONFLICT(card_id) DO UPDATE SET box=?2,due=?3,hits=?4,misses=?5",
                params![id, s.box_, s.due, s.hits, s.misses],
            )
            .map_err(e)?;
        }
        if let Some(ev) = event {
            tx.execute(
                "INSERT INTO events(card_id,correct,retry,delta) VALUES(?1,?2,?3,?4)",
                params![ev.card_id, ev.correct, ev.retry, ev.delta],
            )
            .map_err(e)?;
        }
        tx.commit().map_err(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El teléfono ya tiene una base v1: abrirla con la app nueva debe migrarla sin perder el progreso.
    #[test]
    fn una_base_v1_migra_a_v2_sin_perder_datos() {
        let path = std::env::temp_dir().join(format!("reps-mig-{}.sqlite3", std::process::id()));
        let clean = || {
            for s in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{s}", path.display()));
            }
        };
        clean();
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(include_str!("../migrations/001.sql"))
                .unwrap();
            c.execute("UPDATE profile SET elo=1234 WHERE id=1", [])
                .unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.profile().unwrap().elo, 1234);
        s.add_user_card("u1", "t", "f", "b").unwrap();
        assert_eq!(s.user_cards().unwrap().len(), 1);
        drop(s);
        clean();
    }
}
