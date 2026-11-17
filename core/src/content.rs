//! Contrato del contenido (content/cards.json) y su validación.
use crate::store::Res;
use serde::Deserialize;
use std::collections::HashSet;

/// Marca de hueco en el código de una tarjeta `fill`.
pub const BLANK: &str = "___";

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Kind {
    /// Predice / elige: `answer` es el índice de la opción correcta.
    Choice { options: Vec<String>, answer: usize },
    /// Completa el código: `blanks[i]` es el texto correcto del hueco i; `bank` son las fichas.
    Fill {
        bank: Vec<String>,
        blanks: Vec<String>,
    },
    /// Tarjeta propia: se muestra el dorso y el usuario dice si lo sabía.
    Flip { back: String },
    /// Ordena: `lines` se muestran mezcladas; `solution` lista sus índices en orden correcto.
    Order {
        lines: Vec<String>,
        solution: Vec<usize>,
    },
}

#[derive(Deserialize, Clone, Debug)]
pub struct Card {
    pub id: String,
    pub topic: String,
    pub question: String,
    #[serde(default)]
    pub code: String,
    /// Gancho que se muestra en el botón "Siguiente" cuando esta tarjeta es la próxima.
    pub teaser: String,
    pub explanation: String,
    /// Creada por el usuario desde el lector (no viene de cards.json).
    #[serde(default)]
    pub user: bool,
    #[serde(flatten)]
    pub kind: Kind,
}

/// Duelo contra la IA: encontrar el bug. Se abre tras una ronda perfecta.
#[derive(Deserialize, Clone, Debug)]
pub struct Duel {
    pub topic: String,
    pub title: String,
    pub code: String,
    pub options: Vec<String>,
    pub answer: usize,
    pub explanation: String,
}

#[derive(Deserialize, Debug)]
pub struct Deck {
    pub cards: Vec<Card>,
    pub duels: Vec<Duel>,
}

pub fn load() -> Res<Deck> {
    let deck: Deck = serde_json::from_str(include_str!("../../content/cards.json"))
        .map_err(|e| format!("cards.json: {e}"))?;
    deck.validate()?;
    Ok(deck)
}

impl Deck {
    pub fn validate(&self) -> Res<()> {
        let mut ids = HashSet::new();
        for c in &self.cards {
            let bad = |why: &str| Err(format!("Tarjeta '{}': {why}", c.id));
            if !ids.insert(&c.id) {
                return bad("id repetido");
            }
            if c.teaser.trim().is_empty() || c.explanation.trim().is_empty() {
                return bad("teaser o explicación vacíos");
            }
            match &c.kind {
                Kind::Choice { options, answer } => {
                    if options.len() < 2 || *answer >= options.len() {
                        return bad("opciones o respuesta inválidas");
                    }
                }
                Kind::Fill { bank, blanks } => {
                    if blanks.is_empty() || c.code.matches(BLANK).count() != blanks.len() {
                        return bad("los huecos '___' del código no coinciden con blanks");
                    }
                    if blanks.iter().any(|b| !bank.contains(b)) {
                        return bad("un hueco no está en el banco de fichas");
                    }
                }
                Kind::Flip { back } => {
                    if back.trim().is_empty() {
                        return bad("dorso vacío");
                    }
                }
                Kind::Order { lines, solution } => {
                    let mut seen = vec![false; lines.len()];
                    let perm = solution.len() == lines.len()
                        && solution
                            .iter()
                            .all(|&i| i < seen.len() && !std::mem::replace(&mut seen[i], true));
                    let unique = lines.iter().collect::<HashSet<_>>().len() == lines.len();
                    if lines.len() < 3 || !perm || !unique {
                        return bad("solución no es permutación o líneas repetidas");
                    }
                }
            }
        }
        for d in &self.duels {
            if d.options.len() < 2 || d.answer >= d.options.len() {
                return Err(format!("Duelo '{}': opciones inválidas", d.title));
            }
        }
        if self.cards.is_empty() || self.duels.is_empty() {
            return Err("El mazo no puede estar vacío".into());
        }
        Ok(())
    }
}
