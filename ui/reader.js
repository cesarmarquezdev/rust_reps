// Lector de libros (scroll continuo). Solo textContent: el contenido nunca se interpreta como HTML.
// Usa $, make, highlight y loadHome de app.js. Preferencias y posición: localStorage (comodidad por dispositivo).
(() => {
  const BOOKS = window.BOOKS || [];
  const LS = {
    get: (k, d) => { try { return JSON.parse(localStorage.getItem(k)) ?? d; } catch { return d; } },
    set: (k, v) => { try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* sin almacenamiento: no pasa nada */ } },
  };
  const root = $('reader'), sc = $('scroller'), text = $('text');
  const THEMES = [['original', 'Original'], ['quiet', 'Quiet'], ['paper', 'Paper'], ['bold', 'Bold'], ['calm', 'Calm'], ['focus', 'Focus']];
  const SWATCH = { original: ['#fff', '#1c1c1e'], quiet: ['#3b3b3f', '#dcdce0'], paper: ['#eeebe7', '#2a2826'], bold: ['#fff', '#000'], calm: ['#f3e6cf', '#3a2f22'], focus: ['#fffdf6', '#231f1b'] };
  let prefs = { theme: 'calm', size: 19, font: 'serif', ...LS.get('reader:prefs', {}) };
  let book, chEls = [], index = [], back = null, selText = '', hideT, topic = '';

  /* Resaltado: Rust usa el de app.js; el resto, un tokenizador genérico por lenguaje. */
  const LANG = { rust: 'rust', python: 'python', algoritmos: 'python', datos: 'python', web: 'js', testing: 'python', cpp: 'cpp', metodo: 'python' };
  const KWS = {
    python: 'def class if elif else for while in not and or is return import from as with try except finally raise yield lambda pass break continue None True False self async await del global nonlocal assert',
    sql: 'select from where group by order having join left right inner outer on insert into values update set delete create table primary key foreign references index unique not null default and or in like limit offset as distinct union all case when then else end begin commit rollback count sum avg min max',
    bash: 'cd ls git pip python docker cargo npm sudo echo export if then fi for do done',
    js: 'const let var function return if else for while of in new class extends import export from default async await try catch throw true false null undefined typeof this switch case break continue',
    cpp: 'int double float char bool void auto const static class struct public private return if else for while new delete namespace using include template typename nullptr true false std',
  };
  const COMMENT = { python: '#.*', bash: '#.*', sql: '--.*', js: '//.*', cpp: '//.*' };
  const TOKEN_COMMON = /("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')|(\b\d[\d_.]*\b)|(\b[A-Za-z_]\w*\b)/; // texto | número | palabra
  let lang = 'rust';
  function hl(code, l, parent) {
    if (l === 'rust' || !KWS[l]) return l === 'rust' ? highlight(code, parent) : void parent.append(code);
    const kw = new Set(KWS[l].split(' ')), ci = l === 'sql';
    const re = new RegExp('(' + (COMMENT[l] || '$^') + ')|' + TOKEN_COMMON.source, 'g');
    let last = 0;
    for (const m of code.matchAll(re)) {
      if (m.index > last) parent.append(code.slice(last, m.index));
      const cls = m[1] ? 'c' : m[2] ? 's' : m[3] ? 'n' : kw.has(ci ? m[4].toLowerCase() : m[4]) ? 'k' : /^[A-Z]/.test(m[4]) && !ci ? 't' : '';
      parent.append(cls ? make('span', cls, m[0]) : m[0]);
      last = m.index + m[0].length;
    }
    if (last < code.length) parent.append(code.slice(last));
  }

  const norm = s => s.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
  const frac = () => { const m = sc.scrollHeight - sc.clientHeight; return m > 0 ? sc.scrollTop / m : 0; };
  const goto = f => { sc.scrollTop = f * (sc.scrollHeight - sc.clientHeight); };
  const pctOf = f => Math.round(f * 100) + ' %';
  const current = () => { const y = sc.scrollTop + 90; let i = 0; chEls.forEach((e, k) => { if (e.offsetTop <= y) i = k; }); return i; };
  const toast = msg => { const t = $('toast'); t.textContent = msg; t.hidden = false; clearTimeout(t.timer); t.timer = setTimeout(() => { t.hidden = true; }, 2400); };

  function applyPrefs() {
    root.dataset.theme = prefs.theme;
    root.dataset.font = prefs.font;
    root.style.setProperty('--fs', prefs.size + 'px');
    LS.set('reader:prefs', prefs);
  }

  /* `código` resaltado y **negrita** dentro de un párrafo */
  function inline(s, parent) {
    for (const part of s.split(/(`[^`]+`|\*\*[^*]+\*\*)/)) {
      if (!part) continue;
      if (part[0] === '`') { const c = make('code'); hl(part.slice(1, -1), lang, c); parent.append(c); }
      else if (part.startsWith('**')) parent.append(make('strong', '', part.slice(2, -2)));
      else parent.append(part);
    }
  }

  function render() {
    text.replaceChildren();
    chEls = []; index = [];
    lang = LANG[book.id] || 'rust';
    const cover = make('div', 'cover-page');
    cover.append(make('h1', '', book.title), make('p', '', book.subtitle));
    text.append(cover);
    book.chapters.forEach(c => {
      const s = make('section', 'chapter');
      s.append(make('h2', '', c.title), make('div', 'orn'));
      chEls.push(s);
      for (const k of c.concepts) {
        const e = make('section', 'concept');
        const h3 = make('h3'); inline(k.title, h3); e.append(h3);
        const body = (list, ex) => list.forEach((p, i) => {
          const one = ex && p.t === 'p' && p.x.match(/^`([^`]+)`$/); // ejemplo que es solo código en línea
          if (p.t === 'code' || one) { const pre = make('pre', 'ex'); hl(p.t === 'code' ? p.x : one[1], p.lang || lang, pre); e.append(pre); return; }
          const q = make('p', ex ? 'ex' : '');
          if (ex && i === 0) q.append(make('em', '', 'Ejemplo: '));
          inline(p.x, q);
          e.append(q);
        });
        body(k.que, false);
        body(k.ejemplo, true);
        for (const qa of k.qa) {
          const d = make('details'), sm = make('summary'), ans = make('p');
          sm.append(make('b', '', 'Pon a prueba'), make('br'));
          inline(qa.q, sm);
          inline(qa.a, ans);
          const add = make('button', 'addcard', '＋ Crear tarjeta');
          add.onclick = () => openForm(qa.q, qa.a, c.title);
          d.append(sm, ans, add);
          e.append(d);
        }
        s.append(e);
        index.push({ el: e, ch: c.title, title: k.title.replace(/`/g, ''), raw: [k.title, ...k.que.map(p => p.x), ...k.ejemplo.map(p => p.x), ...k.qa.flatMap(x => [x.q, x.a])].join(' · ').normalize('NFC') });
      }
      text.append(s);
    });
    text.append(make('p', 'fin', '❦ Fin ❦'));
    for (const x of index) x.hay = norm(x.raw);
  }

  /* ── biblioteca ── */
  function shelf() {
    $('shelf').replaceChildren(...BOOKS.map(b => {
      const n = b.chapters.reduce((s, c) => s + c.concepts.length, 0), p = LS.get('reader:pos:' + b.id, 0);
      const t = make('button', 'cover'), art = make('span', 'art', b.title);
      if (b.cover) art.style.background = `linear-gradient(135deg, ${b.cover[0]}, ${b.cover[1]})`;
      t.append(art, make('span', 'bt', b.title), make('span', 'bs', b.subtitle), make('span', 'bs', `${n} conceptos` + (p > 0.005 ? ` · ${pctOf(p)}` : '')));
      t.onclick = () => openBook(b);
      return t;
    }));
  }
  function openLibrary() { root.hidden = false; $('library').hidden = false; $('book').hidden = true; shelf(); }
  $('booksBtn').onclick = openLibrary;
  $('libClose').onclick = () => { root.hidden = true; loadHome(); };

  function openBook(b) {
    book = b; back = null;
    $('bookTitle').textContent = b.title;
    $('backPos').hidden = true;
    closeAll();
    $('library').hidden = true; $('book').hidden = false; $('book').classList.remove('bare');
    applyPrefs();
    render();
    const f = LS.get('reader:pos:' + b.id, 0);
    requestAnimationFrame(() => { goto(f); onScroll(); });
  }
  $('bookClose').onclick = () => { saveNow(); closeAll(); $('mkCard').hidden = true; openLibrary(); };

  /* ── scroll: progreso, guardado y barras que se ocultan al leer ── */
  let lastTop = 0, saveT;
  const saveNow = () => { if (book) LS.set('reader:pos:' + book.id, frac()); };
  function onScroll() {
    $('pct').textContent = `${book.chapters[current()].title} · ${pctOf(frac())}`;
    if (Math.abs(sc.scrollTop - lastTop) > 14) $('book').classList.add('bare');
    lastTop = sc.scrollTop;
    clearTimeout(saveT); saveT = setTimeout(saveNow, 300);
  }
  sc.addEventListener('scroll', onScroll, { passive: true });
  text.addEventListener('click', ev => {
    if (ev.target.closest('summary,button,details') || String(getSelection()).trim()) return;
    $('book').classList.toggle('bare');
  });

  function jump(el) {
    back = frac(); closeAll();
    sc.scrollTop = el.offsetTop - 80;
    el.classList.remove('hit'); void el.offsetWidth; el.classList.add('hit');
    $('backPos').textContent = '↺ ' + pctOf(back); $('backPos').hidden = false;
    $('book').classList.remove('bare');
  }
  $('backPos').onclick = () => { if (back != null) goto(back); back = null; $('backPos').hidden = true; };

  /* ── menú y hojas ── */
  function closeAll() { $('menu').hidden = $('sheet').hidden = $('scrim').hidden = true; }
  $('menuBtn').onclick = () => { const open = $('menu').hidden; closeAll(); $('menu').hidden = $('scrim').hidden = !open; };
  $('scrim').onclick = $('sheetClose').onclick = closeAll;
  function sheet(title, fill) {
    closeAll();
    $('sheetTitle').textContent = title;
    $('sheetBody').replaceChildren();
    fill($('sheetBody'));
    $('sheet').hidden = $('scrim').hidden = false;
  }

  $('mContents').onclick = () => sheet('Contenido', body => {
    book.chapters.forEach((c, i) => {
      const h = make('button', 'row ch', c.title);
      h.onclick = () => jump(chEls[i]);
      body.append(h);
      for (const x of index.filter(x => x.ch === c.title)) {
        const r = make('button', 'row sub', x.title);
        r.onclick = () => jump(x.el);
        body.append(r);
      }
    });
  });

  $('mSearch').onclick = () => sheet('Buscar en el libro', body => {
    const inp = make('input'), out = make('div');
    inp.type = 'search'; inp.placeholder = 'Palabra o concepto…'; inp.setAttribute('aria-label', 'Buscar');
    inp.oninput = () => {
      const q = norm(inp.value.trim());
      out.replaceChildren();
      if (q.length < 2) return;
      const hits = index.filter(x => x.hay.includes(q)).slice(0, 40);
      if (!hits.length) { out.append(make('p', 'none', 'Sin resultados.')); return; }
      for (const x of hits) {
        const r = make('button', 'row', x.title), i = x.hay.indexOf(q);
        r.append(make('small', '', x.ch + ' — …' + x.raw.slice(Math.max(0, i - 40), i + 70).replace(/\s+/g, ' ') + '…'));
        r.onclick = () => jump(x.el);
        out.append(r);
      }
    };
    body.append(inp, out);
    inp.focus();
  });

  $('mThemes').onclick = () => sheet('Temas y ajustes', body => {
    const keep = fn => () => { const f = frac(); fn(); applyPrefs(); goto(f); fill(); };
    const fill = () => {
      body.replaceChildren();
      const size = make('div', 'srow'), font = make('div', 'srow'), grid = make('div', 'themes');
      const sm = make('button', '', 'A'), big = make('button', '', 'A');
      sm.style.fontSize = '14px'; big.style.fontSize = '24px';
      sm.setAttribute('aria-label', 'Letra más pequeña'); big.setAttribute('aria-label', 'Letra más grande');
      sm.onclick = keep(() => { prefs.size = Math.max(14, prefs.size - 1); });
      big.onclick = keep(() => { prefs.size = Math.min(32, prefs.size + 1); });
      size.append(sm, big);
      for (const [id, label] of [['serif', 'Con serifa'], ['sans', 'Sin serifa']]) {
        const b = make('button', '', label);
        b.setAttribute('aria-pressed', String(prefs.font === id));
        b.onclick = keep(() => { prefs.font = id; });
        font.append(b);
      }
      for (const [id, label] of THEMES) {
        const b = make('button', 'theme', 'Aa'), [bg, fg] = SWATCH[id];
        b.style.background = bg; b.style.color = fg;
        b.append(make('small', '', label));
        b.setAttribute('aria-pressed', String(prefs.theme === id));
        b.onclick = keep(() => { prefs.theme = id; });
        grid.append(b);
      }
      body.append(size, font, grid);
    };
    fill();
  });

  /* ── crear tarjeta: seleccionar texto o tocar "Crear tarjeta" en una pregunta ── */
  document.addEventListener('selectionchange', () => {
    if ($('book').hidden || root.hidden) return;
    const s = getSelection(), t = s && !s.isCollapsed && text.contains(s.anchorNode) ? s.toString().trim() : '';
    clearTimeout(hideT);
    if (t) { selText = t.slice(0, 4000); $('mkCard').hidden = false; }
    else hideT = setTimeout(() => { $('mkCard').hidden = true; }, 500);
  });
  $('mkCard').addEventListener('pointerdown', ev => ev.preventDefault()); // no perder la selección al tocar
  $('mkCard').onclick = () => {
    const t = selText;
    getSelection().removeAllRanges();
    $('mkCard').hidden = true;
    openForm('', t, book.chapters[current()].title);
  };

  function openForm(q, a, t) {
    closeAll();
    topic = t;
    $('fq').value = q; $('fa').value = a; $('fTopic').textContent = t; $('fErr').hidden = true;
    $('cardForm').hidden = false;
    (q ? $('fa') : $('fq')).focus();
  }
  $('fSwap').onclick = () => { [$('fq').value, $('fa').value] = [$('fa').value, $('fq').value]; };
  $('fCancel').onclick = () => { $('cardForm').hidden = true; };
  $('fSave').onclick = async () => {
    try {
      await window.__TAURI__.core.invoke('add_card', { topic, front: $('fq').value, back: $('fa').value });
    } catch (e) { $('fErr').textContent = String(e); $('fErr').hidden = false; return; }
    $('cardForm').hidden = true;
    toast('Tarjeta guardada · saldrá en tus rondas');
  };

  applyPrefs();
})();
