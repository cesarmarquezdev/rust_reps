// La UI solo dibuja y manda respuestas. Calificar, mover el Elo y agendar el repaso ocurre en el core Rust.
const $ = id => document.getElementById(id);
const call = async (cmd, args) => {
  try { $('err').hidden = true; return await window.__TAURI__.core.invoke(cmd, args); }
  catch (e) { $('err').textContent = String(e); $('err').hidden = false; }
};
const fmt = s => String(Math.floor(s / 60)).padStart(2, '0') + ':' + String(s % 60).padStart(2, '0');
const signed = v => v > 0 ? '+' + v : String(v).replace('-', '−');
const show = id => { for (const a of ['intro', 'card', 'result', 'duel']) $(a).hidden = a !== id; };
const make = (tag, cls, text) => { const n = document.createElement(tag); if (cls) n.className = cls; if (text != null) n.textContent = text; return n; };

let card, locked, slots, picked, startMs, clock, audio;

/* ── resaltado de sintaxis Rust (solo textContent: nunca se interpreta HTML) ── */
const KW = new Set('fn let mut const if else loop while for in match break continue return use mod pub struct enum impl as true false self super Some None Ok Err'.split(' '));
const TOKEN = /(\/\/.*)|("(?:[^"\\]|\\.)*")|('[a-z_]+)|(\b\d[\d_]*(?:\.\d+)?(?:[iu](?:8|16|32|64|128|size)|f(?:32|64))?\b)|(#!?\[[^\]]*\])|(\b[A-Za-z_]\w*!(?!=))|(\b[A-Za-z_]\w*\b)/g;
function highlight(code, parent) {
  let last = 0;
  for (const m of code.matchAll(TOKEN)) {
    if (m.index > last) parent.append(code.slice(last, m.index));
    const cls = m[1] ? 'c' : m[2] ? 's' : m[3] ? 'l' : m[4] ? 'n' : m[5] ? 'a' : m[6] ? 'm' : KW.has(m[7]) ? 'k' : /^[A-Z]/.test(m[7]) ? 't' : '';
    parent.append(cls ? make('span', cls, m[0]) : m[0]);
    last = m.index + m[0].length;
  }
  if (last < code.length) parent.append(code.slice(last));
}

/* ── sonido ── */
function tone(freqs) {
  if ($('sound').getAttribute('aria-pressed') !== 'true' || !audio || audio.state !== 'running') return;
  const o = audio.createOscillator(), g = audio.createGain(), t = audio.currentTime;
  o.frequency.setValueAtTime(freqs[0], t);
  o.frequency.setValueAtTime(freqs[1], t + 0.07);
  g.gain.setValueAtTime(0.075, t);
  g.gain.exponentialRampToValueAtTime(0.001, t + 0.19);
  o.connect(g).connect(audio.destination);
  o.start();
  o.stop(t + 0.2);
}
$('sound').onclick = async () => {
  const b = $('sound'), on = b.getAttribute('aria-pressed') !== 'true';
  if (on) {
    try { audio ??= new AudioContext(); await audio.resume(); }
    catch { b.textContent = 'Sonido no disponible'; return; }
  }
  b.setAttribute('aria-pressed', String(on));
  b.textContent = on ? 'Sonido activado' : 'Sonido';
};

/* ── pantalla de inicio ── */
function row(dl, k, v) { dl.append(make('dt', '', k), make('dd', '', v)); }
async function loadHome() {
  const h = await call('home');
  if (!h) return;
  $('elo').textContent = h.elo;
  $('rank').textContent = h.rank;
  $('rankLine').textContent = h.next_rank
    ? `Rango: ${h.rank} · te faltan ${h.next_rank[1] - h.elo} Elo para ${h.next_rank[0]}.`
    : `Rango: ${h.rank}. Es el más alto.`;
  const dl = $('homeStats');
  dl.replaceChildren();
  row(dl, 'Por repasar hoy', h.due);
  row(dl, 'Tarjetas nuevas', `${h.fresh} de ${h.total}`);
  if (h.mine) row(dl, 'Mis tarjetas', h.mine);
  row(dl, 'Racha diaria', h.streak ? `${h.streak} ${h.streak === 1 ? 'día' : 'días'}` : '—');
  row(dl, 'Mejor tiempo completo', h.best_secs == null ? '—' : fmt(h.best_secs));
  $('start').textContent = $('again').textContent = h.free ? 'Entrenar gratis · +8 al completar' : 'Jugar ronda · 50 Elo';
}

/* ── ronda ── */
async function startRound() {
  const s = await call('start_round');
  if (!s) return;
  $('elo').textContent = s.elo;
  $('track').hidden = $('finish').hidden = false;
  startMs = Date.now();
  $('timer').textContent = '00:00';
  clearInterval(clock);
  clock = setInterval(() => { $('timer').textContent = fmt(Math.floor((Date.now() - startMs) / 1000)); }, 200);
  show('card');
  renderCard(s.card);
}

const setBar = (passed, total) => { $('bar').style.width = passed / total * 100 + '%'; };

function renderCard(c) {
  card = c;
  locked = false;
  slots = c.kind === 'fill' ? Array(c.slots).fill(null) : [];
  picked = [];
  $('topic').textContent = c.topic;
  $('position').textContent = c.retry ? 'Repesca' : `${c.passed} de ${c.total} acertadas`;
  $('question').textContent = c.question;
  setBar(c.passed, c.total);
  for (const id of ['feedback', 'next', 'check']) $(id).hidden = true;
  $('options').replaceChildren();
  $('builder').hidden = c.kind === 'choice' || c.kind === 'flip';
  $('flip').hidden = c.kind !== 'flip';
  $('back').textContent = c.back;
  $('back').hidden = $('self').hidden = true;
  $('reveal').hidden = false;
  $('code').replaceChildren();
  $('code').classList.remove('hit');

  if (c.kind === 'fill') {
    c.code.split('___').forEach((seg, i) => {
      highlight(seg, $('code'));
      if (i < c.slots) { const b = make('button', 'slot'); b.onclick = () => { if (!locked) { slots[i] = null; renderBuilder(); } }; $('code').append(b); }
    });
    $('builderHint').textContent = 'Toca una ficha para colocarla en el primer hueco libre. Toca un hueco para quitarla.';
  } else {
    highlight(c.code, $('code'));
  }
  if (c.kind === 'order') $('builderHint').textContent = 'Toca las líneas en el orden en que se ejecutarían o se escribirían. Toca una línea elegida para devolverla.';

  if (c.kind === 'choice') {
    c.options.forEach((text, i) => {
      const b = make('button', 'option');
      b.append(make('span', '', 'ABC'[i]), text);
      b.onclick = () => submit({ choice: i });
      $('options').append(b);
    });
  } else if (c.kind !== 'flip') {
    renderBuilder();
  }
  $('question').focus();
}

/* Construye fichas (fill) o líneas (order) según el estado local de la tarjeta. */
function renderBuilder() {
  const bank = $('bank');
  bank.replaceChildren();
  if (card.kind === 'fill') {
    [...$('code').querySelectorAll('.slot')].forEach((b, i) => {
      b.textContent = slots[i] == null ? '' : card.bank[slots[i]];
      b.classList.toggle('filled', slots[i] != null);
    });
    card.bank.forEach((text, j) => {
      const b = make('button', 'chip', text);
      b.disabled = slots.includes(j);
      b.onclick = () => { const k = slots.indexOf(null); if (k >= 0 && !locked) { slots[k] = j; renderBuilder(); } };
      bank.append(b);
    });
    $('picked').hidden = true;
    $('check').disabled = slots.includes(null);
  } else {
    $('picked').hidden = false;
    $('picked').replaceChildren(...picked.map((li, pos) => {
      const b = make('button', 'line');
      b.append(make('span', 'pos', pos + 1));
      highlight(card.lines[li], b);
      b.onclick = () => { if (!locked) { picked.splice(pos, 1); renderBuilder(); } };
      return b;
    }));
    card.lines.forEach((text, li) => {
      if (picked.includes(li)) return;
      const b = make('button', 'line');
      highlight(text, b);
      b.onclick = () => { if (!locked) { picked.push(li); renderBuilder(); } };
      bank.append(b);
    });
    $('check').disabled = picked.length !== card.lines.length;
  }
  $('check').hidden = false;
}

async function submit(answer) {
  if (locked) return;
  locked = true;
  const g = await call('answer', { answer });
  if (!g) { locked = false; return; }
  grade(g, answer);
}

function grade(g, answer) {
  $('elo').textContent = g.elo;
  $('rank').textContent = g.rank;
  setBar(g.passed, g.total);
  $('check').hidden = true;
  if (g.correct) {
    $('elo').classList.remove('pulse'); void $('elo').offsetWidth; $('elo').classList.add('pulse');
    tone([520, 780]);
  } else {
    $('card').classList.remove('shake'); void $('card').offsetWidth; $('card').classList.add('shake');
    tone([300, 190]);
  }
  $('combo').hidden = g.combo < 2;
  $('combo').textContent = `🔥 ×${g.combo}`;

  const heading = make('strong', '', (g.correct ? (g.retry ? 'Repesca superada' : 'Correcto') : 'Respuesta incorrecta') + (g.paid ? ` · ${signed(g.delta)} Elo. ` : '. '));
  const extra = [];
  if (card.kind === 'choice') {
    [...$('options').children].forEach((b, i) => {
      b.disabled = true;
      if (i === g.solution.choice) b.classList.add('right');
      if (!g.correct && i === answer.choice) b.classList.add('wrong');
    });
  } else if (card.kind === 'flip') {
    $('self').hidden = true;
  } else if (card.kind === 'fill') {
    [...$('code').querySelectorAll('.slot')].forEach((b, i) => { b.disabled = true; b.classList.add(g.slot_ok[i] ? 'right' : 'wrong'); });
    [...$('bank').children].forEach(b => { b.disabled = true; });
    if (!g.correct) extra.push(make('pre', 'sol', 'Solución: ' + g.solution.fill.join('  ·  ')));
  } else {
    [...$('picked').children].forEach((b, i) => { b.disabled = true; b.classList.add(g.slot_ok[i] ? 'right' : 'wrong'); });
    [...$('bank').children].forEach(b => { b.disabled = true; });
    if (!g.correct) {
      const pre = make('pre', 'sol');
      highlight(g.solution.order.map(i => card.lines[i]).join('\n'), pre);
      extra.push(make('p', '', 'Orden correcto:'), pre);
    }
  }
  const timing = g.correct ? ' Vuelve cuando toque repasarla.' : ' Vuelve pronto en esta ronda.';
  $('feedback').replaceChildren(heading, (card.kind === 'flip' ? '' : g.explanation) + timing, ...extra);
  $('feedback').hidden = $('next').hidden = false;
  $('next').textContent = g.teaser ? g.teaser + ' →' : 'Ver resultados';
  $('next').focus();
}

$('reveal').onclick = () => { $('reveal').hidden = true; $('back').hidden = $('self').hidden = false; };
$('yes').onclick = () => submit({ flip: true });
$('no').onclick = () => submit({ flip: false });
$('check').onclick = () => submit(card.kind === 'fill' ? { fill: slots } : { order: picked });
$('next').onclick = async () => {
  const s = await call('next_card');
  if (!s) return;
  if (s.step === 'card') renderCard(s.card); else showEnd(s.end);
};
$('finish').onclick = async () => { const e = await call('finish_round'); if (e) showEnd(e); };

function showEnd(e) {
  clearInterval(clock);
  $('timer').textContent = fmt(e.seconds);
  $('elo').textContent = e.elo;
  $('rank').textContent = e.rank;
  $('card').hidden = $('finish').hidden = $('track').hidden = true;
  $('balance').textContent = (e.net < 0 ? `Perdiste ${-e.net} Elo.` : e.net > 0 ? `Ganaste ${e.net} Elo.` : 'Terminaste con el mismo Elo.') + (e.new_best ? ' ¡Nuevo mejor tiempo!' : '');
  $('balance').dataset.loss = String(e.net < 0);
  const dl = $('resultStats');
  dl.replaceChildren();
  row(dl, 'Tiempo total', fmt(e.seconds));
  row(dl, 'Mejor tiempo completo', e.best_secs == null ? '—' : fmt(e.best_secs));
  row(dl, 'Entrada', signed(-e.fee));
  row(dl, 'Cambio por respuestas', signed(e.points));
  row(dl, 'Saldo neto', signed(e.net));
  row(dl, 'Tarjetas acertadas', `${e.passed} / ${e.total}`);
  row(dl, 'Rango', e.rank);
  row(dl, 'Racha diaria', e.streak ? `${e.streak} ${e.streak === 1 ? 'día' : 'días'}` : '—');
  $('duelOpen').hidden = !e.duel;
  show('result');
  $('done').focus();
  loadHome();
}

/* ── duelo contra la IA ── */
$('duelOpen').onclick = async () => {
  const d = await call('open_duel');
  if (!d) return;
  show('duel');
  $('duelTitle').textContent = d.title;
  $('duelCode').replaceChildren();
  highlight(d.code, $('duelCode'));
  $('duelFeedback').textContent = '';
  const box = $('duelOptions');
  box.replaceChildren();
  d.options.forEach((text, i) => {
    const b = make('button', 'option', text);
    b.onclick = async () => {
      if (b.disabled) return;
      const r = await call('answer_duel', { choice: i });
      if (!r) return;
      [...box.children].forEach((c, j) => { c.disabled = true; if (j === r.answer) c.classList.add('right'); if (j === i && !r.correct) c.classList.add('wrong'); });
      if (r.correct) tone([520, 780]);
      $('duelFeedback').textContent = (r.correct ? 'Bug encontrado. ' : 'Este es el fallo. ') + r.explanation;
      $('duelBack').focus();
    };
    box.append(b);
  });
  $('duelTitle').focus();
};
$('duelBack').onclick = () => { show('result'); $('duelOpen').focus(); };
$('start').onclick = $('again').onclick = startRound;

// Teclas 1-3 eligen opción en tarjetas de elegir.
document.addEventListener('keydown', ev => {
  if (ev.ctrlKey || ev.metaKey || ev.altKey || $('card').hidden || locked || !card || card.kind !== 'choice') return;
  const b = $('options').children[Number(ev.key) - 1];
  if (b) b.click();
});

loadHome();
