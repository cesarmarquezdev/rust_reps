//! Reglas del juego. La UI solo dibuja: aquí se califica, se mueve el Elo y se agenda el repaso.
use crate::content::{self, Card, Deck, Kind};
use crate::store::{CardState, Event, Profile, Res, Store};
use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    time::Instant,
};

pub const ENTRY: i64 = 50; // coste de una ronda
pub const HIT: i64 = 20; // acierto
pub const MISS: i64 = 12; // error
pub const RETRY: i64 = 8; // acierto en repesca
pub const FREE_BONUS: i64 = 8; // completar una ronda gratis
pub const ROUND: usize = 5; // tarjetas por ronda
/// Días hasta el próximo repaso según la caja (0 = vuelve hoy).
const STEPS: [i64; 6] = [0, 1, 3, 7, 14, 30];
const RANKS: [(i64, &str); 5] = [
    (0, "Novato del borrow checker"),
    (900, "Aprendiz de Cargo"),
    (1100, "Rustacean"),
    (1300, "Auditor de IA"),
    (1500, "Maestro del ownership"),
];

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Answer {
    Choice(usize),
    Fill(Vec<usize>),
    Order(Vec<usize>),
    /// Tarjeta propia: true = "lo sabía".
    Flip(bool),
}

#[derive(Serialize)]
pub struct Home {
    pub elo: i64,
    pub rank: &'static str,
    pub next_rank: Option<(&'static str, i64)>,
    pub streak: i64,
    pub due: usize,
    pub fresh: usize,
    pub total: usize,
    pub mine: usize,
    pub best_secs: Option<i64>,
    pub rounds: i64,
    pub free: bool,
}

#[derive(Serialize)]
pub struct CardView {
    pub topic: String,
    pub question: String,
    pub code: String,
    pub kind: &'static str,
    pub options: Vec<String>,
    pub bank: Vec<String>,
    pub slots: usize,
    pub lines: Vec<String>,
    /// Solo en tarjetas propias (flip): el dorso, que el usuario se autocalifica.
    pub back: String,
    pub retry: bool,
    pub passed: usize,
    pub total: usize,
}

#[derive(Serialize)]
pub struct Started {
    pub elo: i64,
    pub fee: i64,
    pub card: CardView,
}

#[derive(Serialize)]
pub struct Graded {
    pub correct: bool,
    pub retry: bool,
    pub delta: i64,
    pub elo: i64,
    pub rank: &'static str,
    pub passed: usize,
    pub total: usize,
    pub combo: u32,
    pub explanation: String,
    /// La solución: {"choice":i} | {"fill":[texto]} | {"order":[i]}.
    pub solution: Value,
    /// Acierto por hueco (fill) o por posición (order); vacío en choice.
    pub slot_ok: Vec<bool>,
    pub teaser: Option<String>,
    pub paid: bool,
}

#[derive(Serialize)]
pub struct Ended {
    pub seconds: i64,
    pub best_secs: Option<i64>,
    pub new_best: bool,
    pub fee: i64,
    pub points: i64,
    pub net: i64,
    pub elo: i64,
    pub rank: &'static str,
    pub passed: usize,
    pub total: usize,
    pub streak: i64,
    pub duel: bool,
}

#[derive(Serialize)]
#[serde(tag = "step", rename_all = "lowercase")]
pub enum Step {
    Card { card: CardView },
    End { end: Ended },
}

#[derive(Serialize)]
pub struct DuelView {
    pub title: String,
    pub code: String,
    pub options: Vec<String>,
}

#[derive(Serialize)]
pub struct DuelResult {
    pub correct: bool,
    pub answer: usize,
    pub explanation: String,
}

struct Round {
    queue: VecDeque<(usize, bool)>, // (índice de tarjeta, es repesca)
    current: Option<(usize, bool)>,
    answered: bool,
    passed: usize,
    total: usize,
    fee: i64,
    start_elo: i64,
    missed: Option<usize>,
    combo: u32,
    started: Instant,
    cards: Vec<usize>,
}

struct Last {
    missed: Option<usize>,
    cards: Vec<usize>,
    perfect: bool,
}

pub struct Engine {
    store: Store,
    deck: Deck,
    profile: Profile,
    states: HashMap<String, CardState>,
    round: Option<Round>,
    last: Option<Last>,
    duel: Option<usize>,
    today_override: Option<NaiveDate>, // solo para tests
}

fn rank(elo: i64) -> &'static str {
    RANKS
        .iter()
        .rev()
        .find(|r| elo >= r.0)
        .map_or(RANKS[0].1, |r| r.1)
}

fn view(c: &Card, retry: bool, passed: usize, total: usize) -> CardView {
    let mut back = String::new();
    let (kind, options, bank, slots, lines) = match &c.kind {
        Kind::Choice { options, .. } => ("choice", options.clone(), vec![], 0, vec![]),
        Kind::Fill { bank, blanks } => ("fill", vec![], bank.clone(), blanks.len(), vec![]),
        Kind::Order { lines, .. } => ("order", vec![], vec![], 0, lines.clone()),
        Kind::Flip { back: b } => {
            back = b.clone();
            ("flip", vec![], vec![], 0, vec![])
        }
    };
    CardView {
        topic: c.topic.clone(),
        question: c.question.clone(),
        code: c.code.clone(),
        kind,
        options,
        bank,
        slots,
        lines,
        back,
        retry,
        passed,
        total,
    }
}

/// Califica sin tocar el estado. Una respuesta mal formada es un error, no un fallo del jugador.
fn grade(kind: &Kind, a: &Answer) -> Res<(bool, Vec<bool>, Value)> {
    match (kind, a) {
        (Kind::Choice { options, answer }, Answer::Choice(i)) => {
            if *i >= options.len() {
                return Err("Opción inválida".into());
            }
            Ok((i == answer, vec![], json!({ "choice": answer })))
        }
        (Kind::Fill { bank, blanks }, Answer::Fill(v)) => {
            if v.len() != blanks.len() || v.iter().any(|&i| i >= bank.len()) {
                return Err("Respuesta de huecos inválida".into());
            }
            let ok: Vec<bool> = v.iter().zip(blanks).map(|(&i, b)| &bank[i] == b).collect();
            Ok((ok.iter().all(|&x| x), ok, json!({ "fill": blanks })))
        }
        (Kind::Order { lines, solution }, Answer::Order(v)) => {
            let mut seen = vec![false; lines.len()];
            let valid = v.len() == lines.len()
                && v.iter()
                    .all(|&i| i < seen.len() && !std::mem::replace(&mut seen[i], true));
            if !valid {
                return Err("Orden inválido".into());
            }
            let ok: Vec<bool> = v.iter().zip(solution).map(|(a, b)| a == b).collect();
            Ok((v == solution, ok, json!({ "order": solution })))
        }
        (Kind::Flip { back }, Answer::Flip(knew)) => Ok((*knew, vec![], json!({ "flip": back }))),
        _ => Err("La respuesta no corresponde al tipo de tarjeta".into()),
    }
}

fn user_card((id, topic, front, back): (String, String, String, String)) -> Card {
    Card {
        id,
        topic,
        question: front,
        code: String::new(),
        teaser: "Una tarjeta tuya".into(),
        explanation: back.clone(),
        user: true,
        kind: Kind::Flip { back },
    }
}

impl Engine {
    /// Crea una tarjeta propia (desde el lector). Sale primero entre las nuevas.
    pub fn add_card(&mut self, topic: &str, front: &str, back: &str) -> Res<()> {
        let (topic, front, back) = (topic.trim(), front.trim(), back.trim());
        if front.is_empty() || back.is_empty() {
            return Err("La pregunta y la respuesta no pueden estar vacías".into());
        }
        if front.len() > 2000 || back.len() > 4000 {
            return Err("La tarjeta es demasiado larga".into());
        }
        let id = format!("u{}", Local::now().timestamp_millis());
        let topic = if topic.is_empty() {
            "Mis tarjetas"
        } else {
            topic
        };
        self.store.add_user_card(&id, topic, front, back)?;
        self.deck
            .cards
            .push(user_card((id, topic.into(), front.into(), back.into())));
        Ok(())
    }

    pub fn new(store: Store) -> Res<Self> {
        let profile = store.profile()?;
        let states = store.cards()?;
        let mut deck = content::load()?;
        deck.cards
            .extend(store.user_cards()?.into_iter().map(user_card));
        Ok(Self {
            store,
            deck,
            profile,
            states,
            round: None,
            last: None,
            duel: None,
            today_override: None,
        })
    }

    fn today(&self) -> NaiveDate {
        self.today_override
            .unwrap_or_else(|| Local::now().date_naive())
    }

    fn due_date(&self, id: &str) -> Option<NaiveDate> {
        self.states.get(id).and_then(|s| s.due.parse().ok())
    }

    pub fn home(&self) -> Home {
        let today = self.today();
        let seen = self
            .deck
            .cards
            .iter()
            .filter(|c| self.states.contains_key(&c.id))
            .count();
        let due = self
            .deck
            .cards
            .iter()
            .filter(|c| self.due_date(&c.id).is_some_and(|d| d <= today))
            .count();
        let elo = self.profile.elo;
        // La racha diaria se pierde si no jugaste ayer ni hoy.
        let alive = self.profile.last_day.as_deref().is_some_and(|d| {
            d == today.to_string() || d == (today - Duration::days(1)).to_string()
        });
        Home {
            elo,
            rank: rank(elo),
            next_rank: RANKS.iter().find(|r| r.0 > elo).map(|r| (r.1, r.0)),
            streak: if alive { self.profile.streak } else { 0 },
            due,
            fresh: self.deck.cards.len() - seen,
            total: self.deck.cards.len(),
            mine: self.deck.cards.iter().filter(|c| c.user).count(),
            best_secs: self.profile.best_secs,
            rounds: self.profile.rounds,
            free: elo < ENTRY,
        }
    }

    /// Vencidas primero (la más atrasada antes), luego nuevas, luego las que vencen más pronto.
    fn pick(&self) -> Vec<usize> {
        let today = self.today();
        let (mut due, mut fresh, mut later) = (vec![], vec![], vec![]);
        for (i, c) in self.deck.cards.iter().enumerate() {
            match self.due_date(&c.id) {
                None => fresh.push(i),
                Some(d) if d <= today => due.push((d, i)),
                Some(d) => later.push((d, i)),
            }
        }
        fresh.sort_by_key(|&i| !self.deck.cards[i].user); // las tuyas primero
        due.sort();
        later.sort();
        due.into_iter()
            .map(|x| x.1)
            .chain(fresh)
            .chain(later.into_iter().map(|x| x.1))
            .take(ROUND)
            .collect()
    }

    pub fn start(&mut self) -> Res<Started> {
        let cards = self.pick();
        let fee = if self.profile.elo >= ENTRY { ENTRY } else { 0 };
        let start_elo = self.profile.elo;
        self.profile.elo -= fee;
        self.store.record(&self.profile, None, None)?; // la entrada se cobra al empezar
        self.last = None;
        self.duel = None;
        self.round = Some(Round {
            queue: cards.iter().map(|&i| (i, false)).collect(),
            current: None,
            answered: false,
            passed: 0,
            total: cards.len(),
            fee,
            start_elo,
            missed: None,
            combo: 0,
            started: Instant::now(),
            cards,
        });
        let card = self.advance().ok_or("No hay tarjetas")?;
        Ok(Started {
            elo: self.profile.elo,
            fee,
            card,
        })
    }

    fn advance(&mut self) -> Option<CardView> {
        let r = self.round.as_mut()?;
        let item = r.queue.pop_front()?;
        r.current = Some(item);
        r.answered = false;
        Some(view(&self.deck.cards[item.0], item.1, r.passed, r.total))
    }

    pub fn answer(&mut self, a: Answer) -> Res<Graded> {
        let today = self.today();
        let r = self.round.as_mut().ok_or("No hay ronda activa")?;
        if r.answered {
            return Err("Ya respondiste esta tarjeta".into());
        }
        let (idx, retry) = r.current.ok_or("No hay tarjeta activa")?;
        let card = &self.deck.cards[idx];
        let (correct, slot_ok, solution) = grade(&card.kind, &a)?;
        r.answered = true;

        let before = self.profile.elo;
        let paid = r.fee > 0;
        if paid {
            let d = match (correct, retry) {
                (true, true) => RETRY,
                (true, false) => HIT,
                (false, _) => -MISS,
            };
            self.profile.elo = (before + d).max(0);
        }
        let delta = self.profile.elo - before;

        // Solo el primer intento mueve la caja; la repesca no cuenta como recuerdo.
        let state = (!retry).then(|| {
            let s = self
                .states
                .entry(card.id.clone())
                .or_insert_with(|| CardState {
                    box_: 0,
                    due: today.to_string(),
                    hits: 0,
                    misses: 0,
                });
            if correct {
                s.box_ = (s.box_ + 1).min(5);
                s.hits += 1;
            } else {
                s.box_ = 0;
                s.misses += 1;
            }
            s.due = (today + Duration::days(STEPS[s.box_ as usize])).to_string();
            s.clone()
        });

        if correct {
            r.passed += 1;
            r.combo += 1;
        } else {
            r.combo = 0;
            r.missed.get_or_insert(idx);
            let at = r.queue.len().min(2);
            r.queue.insert(at, (idx, true));
        }
        self.store.record(
            &self.profile,
            state.as_ref().map(|s| (card.id.as_str(), s)),
            Some(Event {
                card_id: &card.id,
                correct,
                retry,
                delta,
            }),
        )?;
        Ok(Graded {
            correct,
            retry,
            delta,
            elo: self.profile.elo,
            rank: rank(self.profile.elo),
            passed: r.passed,
            total: r.total,
            combo: r.combo,
            explanation: card.explanation.clone(),
            solution,
            slot_ok,
            teaser: r
                .queue
                .front()
                .map(|&(i, _)| self.deck.cards[i].teaser.clone()),
            paid,
        })
    }

    pub fn next_card(&mut self) -> Res<Step> {
        let r = self.round.as_ref().ok_or("No hay ronda activa")?;
        if r.current.is_some() && !r.answered {
            return Err("Responde la tarjeta antes de continuar".into());
        }
        match self.advance() {
            Some(card) => Ok(Step::Card { card }),
            None => Ok(Step::End {
                end: self.finish()?,
            }),
        }
    }

    pub fn finish(&mut self) -> Res<Ended> {
        let r = self.round.take().ok_or("No hay ronda activa")?;
        let today = self.today();
        let seconds = r.started.elapsed().as_secs() as i64;
        let perfect = r.passed == r.total;
        let new_best = perfect && self.profile.best_secs.is_none_or(|b| seconds < b);
        if new_best {
            self.profile.best_secs = Some(seconds);
        }
        if perfect && r.fee == 0 {
            self.profile.elo += FREE_BONUS;
        }
        if r.passed > 0 && self.profile.last_day.as_deref() != Some(&today.to_string()) {
            let yesterday = (today - Duration::days(1)).to_string();
            let continued = self.profile.last_day.as_deref() == Some(&yesterday);
            self.profile.streak = if continued {
                self.profile.streak + 1
            } else {
                1
            };
            self.profile.last_day = Some(today.to_string());
        }
        self.profile.rounds += 1;
        self.store.record(&self.profile, None, None)?;
        let net = self.profile.elo - r.start_elo;
        self.last = Some(Last {
            missed: r.missed,
            cards: r.cards,
            perfect,
        });
        Ok(Ended {
            seconds,
            best_secs: self.profile.best_secs,
            new_best,
            fee: r.fee,
            points: net + r.fee,
            net,
            elo: self.profile.elo,
            rank: rank(self.profile.elo),
            passed: r.passed,
            total: r.total,
            streak: self.profile.streak,
            duel: perfect,
        })
    }

    /// El duelo trata el tema de la tarjeta que fallaste (o de cualquiera de la ronda).
    pub fn duel(&mut self) -> Res<DuelView> {
        let last = self
            .last
            .as_ref()
            .filter(|l| l.perfect)
            .ok_or("El duelo se abre al completar una ronda")?;
        let i = last
            .missed
            .iter()
            .chain(&last.cards)
            .find_map(|&c| {
                let topic = &self.deck.cards[c].topic;
                self.deck.duels.iter().position(|d| &d.topic == topic)
            })
            .unwrap_or(0);
        self.duel = Some(i);
        let d = &self.deck.duels[i];
        Ok(DuelView {
            title: d.title.clone(),
            code: d.code.clone(),
            options: d.options.clone(),
        })
    }

    pub fn duel_answer(&mut self, choice: usize) -> Res<DuelResult> {
        let d = &self.deck.duels[self.duel.ok_or("Abre primero el duelo")?];
        if choice >= d.options.len() {
            return Err("Opción inválida".into());
        }
        Ok(DuelResult {
            correct: choice == d.answer,
            answer: d.answer,
            explanation: d.explanation.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BLANK;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn engine() -> Engine {
        let mut e = Engine::new(Store::memory().unwrap()).unwrap();
        e.today_override = Some(day(16));
        e
    }

    /// Respuesta correcta (o deliberadamente mala) de la tarjeta actual.
    fn answer_for(e: &Engine, ok: bool) -> Answer {
        let idx = e.round.as_ref().unwrap().current.unwrap().0;
        match &e.deck.cards[idx].kind {
            Kind::Choice { options, answer } => Answer::Choice(if ok {
                *answer
            } else {
                (answer + 1) % options.len()
            }),
            Kind::Fill { bank, blanks } => {
                let mut v: Vec<usize> = blanks
                    .iter()
                    .map(|b| bank.iter().position(|x| x == b).unwrap())
                    .collect();
                if !ok {
                    v[0] = bank.iter().position(|x| x != &blanks[0]).unwrap();
                }
                Answer::Fill(v)
            }
            Kind::Order { solution, .. } => {
                let mut v = solution.clone();
                if !ok {
                    v.swap(0, 1);
                }
                Answer::Order(v)
            }
            Kind::Flip { .. } => Answer::Flip(ok),
        }
    }

    fn play(e: &mut Engine, first_ok: bool) -> Ended {
        e.start().unwrap();
        let mut ok = first_ok;
        loop {
            let a = answer_for(e, ok);
            e.answer(a).unwrap();
            ok = true;
            if let Step::End { end } = e.next_card().unwrap() {
                return end;
            }
        }
    }

    #[test]
    fn el_mazo_es_valido_y_los_duelos_tienen_tema() {
        let deck = content::load().unwrap();
        assert!(deck.cards.len() >= 10);
        for d in &deck.duels {
            assert!(
                deck.cards.iter().any(|c| c.topic == d.topic),
                "duelo sin tarjetas: {}",
                d.topic
            );
        }
        assert!(deck.cards.iter().filter(|c| c.code.contains(BLANK)).count() >= 3);
    }

    /// Anti-patrón: si la correcta casi siempre es la A, se gana sin saber Rust.
    #[test]
    fn la_posicion_de_la_respuesta_no_delata() {
        let deck = content::load().unwrap();
        let mut pos = [0usize; 3];
        for c in &deck.cards {
            match &c.kind {
                Kind::Choice { answer, .. } => pos[*answer] += 1,
                // Las fichas no deben venir ya en el orden de la solución.
                Kind::Fill { bank, blanks } => {
                    assert!(
                        !blanks.iter().enumerate().all(|(i, b)| &bank[i] == b),
                        "banco resuelto: {}",
                        c.id
                    )
                }
                Kind::Order { solution, .. } => {
                    assert!(
                        !solution.iter().enumerate().all(|(i, &s)| i == s),
                        "orden resuelto: {}",
                        c.id
                    )
                }
                Kind::Flip { .. } => {}
            }
        }
        let n: usize = pos.iter().sum();
        assert!(
            pos.iter().all(|&p| p * 2 <= n),
            "respuestas sesgadas: {pos:?}"
        );
        for d in &deck.duels {
            assert!(d.options.len() == 3);
        }
    }

    #[test]
    fn ronda_perfecta_cobra_entrada_y_agenda_para_manana() {
        let mut e = engine();
        let end = play(&mut e, true);
        assert_eq!((end.passed, end.total, end.fee), (ROUND, ROUND, 50));
        assert_eq!(end.elo, 1000 - 50 + 5 * HIT);
        assert_eq!(end.net, 50);
        assert!(end.duel && end.new_best);
        assert_eq!(e.states.len(), ROUND);
        assert!(
            e.states
                .values()
                .all(|s| s.box_ == 1 && s.due == day(17).to_string())
        );
        assert_eq!(e.home().streak, 1);
    }

    #[test]
    fn fallo_resta_y_la_repesca_devuelve_la_tarjeta() {
        let mut e = engine();
        e.start().unwrap();
        let g = e.answer(answer_for(&e, false)).unwrap();
        assert!(!g.correct && g.delta == -MISS && g.teaser.is_some());
        let missed = e.round.as_ref().unwrap().current.unwrap().0;
        let mut seen_retry = false;
        loop {
            if let Step::End { end } = e.next_card().unwrap() {
                assert_eq!(end.elo, 1000 - 50 - MISS + RETRY + 4 * HIT);
                assert!(end.duel, "la ronda se completó con repesca");
                break;
            }
            let cur = e.round.as_ref().unwrap().current.unwrap();
            seen_retry |= cur == (missed, true);
            e.answer(answer_for(&e, true)).unwrap();
        }
        assert!(seen_retry);
        let s = &e.states[&e.deck.cards[missed].id];
        assert_eq!((s.box_, s.misses, s.due.as_str()), (0, 1, "2026-11-16"));
        // El duelo sale del tema de la tarjeta fallada.
        let topic = e.deck.cards[missed].topic.clone();
        e.duel().unwrap();
        let d = &e.deck.duels[e.duel.unwrap()];
        assert!(d.topic == topic || !e.deck.duels.iter().any(|d| d.topic == topic));
    }

    #[test]
    fn sin_elo_suficiente_se_entrena_gratis() {
        let mut e = engine();
        e.profile.elo = 30;
        let end = play(&mut e, false);
        assert_eq!(end.fee, 0);
        assert_eq!(end.elo, 30 + FREE_BONUS, "fallar no resta en ronda gratis");
    }

    #[test]
    fn el_elo_nunca_baja_de_cero() {
        let mut e = engine();
        e.profile.elo = ENTRY; // entra, queda en 0 y falla
        e.start().unwrap();
        let g = e.answer(answer_for(&e, false)).unwrap();
        assert_eq!((g.elo, g.delta), (0, 0));
    }

    #[test]
    fn repaso_espaciado_las_vencidas_van_primero() {
        let mut e = engine();
        play(&mut e, true);
        let first: Vec<usize> = e
            .states
            .keys()
            .map(|id| e.deck.cards.iter().position(|c| &c.id == id).unwrap())
            .collect();
        // Mismo día: lo visto espera, salen tarjetas nuevas.
        assert!(e.pick().iter().all(|i| !first.contains(i)));
        // Al día siguiente vuelven exactamente las 5 vencidas.
        e.today_override = Some(day(17));
        let mut got = e.pick();
        let mut want = first;
        got.sort();
        want.sort();
        assert_eq!(got, want);
        assert_eq!(e.home().due, ROUND);
        // Un día después de saltarte la racha, vuelve a cero.
        e.today_override = Some(day(19));
        assert_eq!(e.home().streak, 0);
    }

    #[test]
    fn respuestas_mal_formadas_son_error_y_no_gastan_la_tarjeta() {
        let mut e = engine();
        e.start().unwrap();
        assert!(e.answer(Answer::Choice(99)).is_err());
        assert!(e.answer(Answer::Fill(vec![])).is_err());
        assert!(e.answer(Answer::Order(vec![0, 0, 0])).is_err());
        assert!(e.next_card().is_err(), "no se avanza sin responder");
        let a = answer_for(&e, true);
        assert!(e.answer(a).unwrap().correct);
        let a = answer_for(&e, true);
        assert!(e.answer(a).is_err(), "doble envío");
    }

    #[test]
    fn tarjetas_propias_se_guardan_salen_primero_y_se_autocalifican() {
        let path = std::env::temp_dir().join(format!("reps-user-{}.sqlite3", std::process::id()));
        let clean = || {
            for s in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{s}", path.display()));
            }
        };
        clean();
        {
            let mut e = Engine::new(Store::open(&path).unwrap()).unwrap();
            e.today_override = Some(day(16));
            assert!(e.add_card("Cap", "  ", "x").is_err());
            e.add_card("Cap", "¿Qué es X?", "Y").unwrap();
            assert_eq!(e.home().mine, 1);
        }
        let mut e = Engine::new(Store::open(&path).unwrap()).unwrap();
        e.today_override = Some(day(16));
        assert_eq!(e.home().mine, 1, "sobrevive al reabrir");
        let s = e.start().unwrap();
        assert_eq!((s.card.kind, s.card.back.as_str()), ("flip", "Y"));
        let g = e.answer(Answer::Flip(true)).unwrap();
        assert!(g.correct && g.delta == HIT);
        assert!(e.answer(Answer::Choice(0)).is_err());
        drop(e);
        clean();
    }

    #[test]
    fn el_progreso_sobrevive_al_reabrir_la_base() {
        let path = std::env::temp_dir().join(format!("reps-test-{}.sqlite3", std::process::id()));
        let clean = || {
            for s in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{s}", path.display()));
            }
        };
        clean();
        let elo = {
            let mut e = Engine::new(Store::open(&path).unwrap()).unwrap();
            e.today_override = Some(day(16));
            play(&mut e, true).elo
        };
        let e = Engine::new(Store::open(&path).unwrap()).unwrap();
        assert_eq!((e.profile.elo, e.states.len()), (elo, ROUND));
        drop(e);
        clean();
    }
}
