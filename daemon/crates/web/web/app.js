'use strict';
// OrcaDeck editor. Plain DOM, no build step. All data goes through h()/textContent, never innerHTML.

const $ = (s, el = document) => el.querySelector(s);
const PROPS = new Set(['value', 'checked', 'selected', 'disabled', 'hidden', 'open']);
function h(tag, props, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (v == null || v === false) continue;
    if (k === 'class') el.className = v;
    else if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else if (PROPS.has(k)) el[k] = v;
    else el.setAttribute(k, v === true ? '' : v);
  }
  for (const c of kids.flat(9)) if (c != null && c !== false) el.append(c.nodeType ? c : String(c));
  return el;
}

// replaceChildren() would print `false`/`null` from optional `cond && node` entries; drop them.
const fill = (el, items) => el.replaceChildren(...items.flat().filter((x) => x != null && x !== false && x !== ''));

const T_ACTION = 'application/x-orcadeck-action', T_CELL = 'application/x-orcadeck-cell', T_SCENE = 'application/x-orcadeck-scene';
const ROTATE = 'com.hotspot.streamdock.profile.rotate';
const S = { scenes: [], id: null, scene: null, groups: [], index: new Map(), sel: null, draft: null, dirty: false, plugins: null, psel: null, piReq: 0, piFrame: null, piCreds: null };

async function api(method, path, body) {
  const init = { method, headers: {} };
  if (body !== undefined) { init.headers['Content-Type'] = 'application/json'; init.body = JSON.stringify(body); }
  const r = await fetch('/api/' + path, init);
  if (r.status === 401) { $('#locked').hidden = false; throw new Error('Sesión no iniciada'); }
  const j = await r.json().catch(() => ({}));
  if (!r.ok) { const e = new Error(j.error || `Error ${r.status}`); e.status = r.status; throw e; }
  return j;
}

function status(msg, err) {
  for (const el of document.querySelectorAll('.status')) {
    el.textContent = msg || '';
    el.classList.toggle('err', !!err);
  }
}
const fail = (e) => status(e.message, true);
const q = (o) => new URLSearchParams(o).toString();

// ---------- images ----------
function stateOf(a) { return (a.States || [])[a.State || 0] || (a.States || [])[0] || {}; }
function cellImg(a) {
  return '/api/image?' + q({ scene: S.id, uuid: a.UUID, img: stateOf(a).Image || '', state: a.State || 0 });
}
function img(src, alt, fallbackText, cls) {
  if (/[?&]img=(&|$)/.test(src) && !/[?&]state=/.test(src)) return h('span', { class: cls || 'ph' }, fallbackText || '');
  const i = h('img', { src, alt: alt || '', loading: 'lazy', decoding: 'async' });
  i.addEventListener('error', () => i.replaceWith(h('span', { class: cls || 'ph' }, fallbackText || '')), { once: true });
  return i;
}

// ---------- scenes ----------
const isAkp = (doc) => (doc.DeviceUUID || '') === 'AKP153E';
const sceneName = (id) => (S.scenes.find((s) => s.id === id) || {}).name || id;

async function loadScenes() {
  S.scenes = await api('GET', 'scenes');
  renderScenes();
}

async function openScene(id, landed) {
  S.scene = await api('GET', 'scenes/' + encodeURIComponent(id));
  if (S.id !== id) { S.sel = null; }
  S.id = id;
  renderScenes();
  renderDeck(landed);
  renderInspector();
}

function renderScenes() {
  const ol = $('#scene-tabs');
  ol.replaceChildren();
  const listed = S.scenes.filter((s) => s.listed);
  for (const s of listed) {
    const b = h('button', {
      type: 'button', class: 'tab', draggable: 'true', 'aria-current': s.id === S.id ? 'true' : 'false',
      title: s.current ? 'Escena que muestra el dispositivo ahora' : null,
      onclick: () => openScene(s.id).catch(fail),
    }, h('span', null, s.name || 'Sin nombre'), s.current && h('span', { class: 'live', 'aria-label': 'en el dispositivo' }));
    b.addEventListener('dragstart', (e) => { e.dataTransfer.setData(T_SCENE, s.id); e.dataTransfer.effectAllowed = 'move'; b.classList.add('dragging'); });
    b.addEventListener('dragend', () => b.classList.remove('dragging'));
    b.addEventListener('dragover', (e) => {
      const t = e.dataTransfer.types;
      if (t.includes(T_SCENE) || (t.includes(T_CELL) && s.id !== S.id)) { e.preventDefault(); b.classList.add('drop-before'); }
    });
    b.addEventListener('dragleave', () => b.classList.remove('drop-before'));
    b.addEventListener('drop', async (e) => {
      e.preventDefault();
      b.classList.remove('drop-before');
      const moved = e.dataTransfer.getData(T_SCENE), cell = e.dataTransfer.getData(T_CELL);
      try {
        if (moved && moved !== s.id) {
          const order = listed.map((x) => x.id).filter((x) => x !== moved);
          order.splice(order.indexOf(s.id), 0, moved);
          await api('PUT', 'scenes/order', order);
          await loadScenes();
          status('Orden de escenas guardado');
        } else if (cell) {
          await api('POST', `scenes/${S.id}/cells/${cell}/move`, { to: cell, scene: s.id, copy: e.ctrlKey || e.altKey });
          S.sel = null;
          await loadScenes();
          await openScene(S.id);
          status(`${e.ctrlKey || e.altKey ? 'Copiada' : 'Movida'} a ${s.name} (misma posición)`);
        }
      } catch (err) { fail(err); }
    });
    ol.append(h('li', null, b));
  }
  const others = S.scenes.filter((s) => !s.listed);
  const sel = $('#other-scenes');
  sel.replaceChildren(h('option', { value: '', disabled: true, selected: !others.some((s) => s.id === S.id) }, `Otras escenas (${others.length})`),
    ...others.map((s) => h('option', { value: s.id, selected: s.id === S.id }, `${s.name || 'Sin nombre'} · ${s.device || '?'}`)));
  sel.hidden = others.length === 0;
}

$('#other-scenes').addEventListener('change', (e) => e.target.value && openScene(e.target.value).catch(fail));

$('#new-scene').addEventListener('click', function () {
  const tools = this.parentElement, saved = [...tools.children];
  const input = h('input', { type: 'text', placeholder: 'Nombre de la escena', 'aria-label': 'Nombre de la nueva escena', class: 'btn' });
  const done = () => tools.replaceChildren(...saved);
  const create = async () => {
    const name = input.value.trim();
    if (!name) return input.focus();
    try {
      const r = await api('POST', 'scenes', { name });
      done();
      await loadScenes();
      await openScene(r.id);
      status(`Escena «${name}» creada`);
    } catch (e) { fail(e); }
  };
  input.addEventListener('keydown', (e) => { if (e.key === 'Enter') create(); if (e.key === 'Escape') done(); });
  tools.replaceChildren(input, h('button', { type: 'button', class: 'btn primary', onclick: create }, 'Crear'),
    h('button', { type: 'button', class: 'btn', onclick: done }, 'Cancelar'));
  input.focus();
});

// ---------- device grid ----------
function geometry(doc) {
  if (isAkp(doc)) return { cols: 5, rows: 3, side: true };
  let cols = 3, rows = 3;
  for (const k of Object.keys(doc.Actions || {})) {
    const [c, r] = k.split(',').map(Number);
    cols = Math.max(cols, c + 1); rows = Math.max(rows, r + 1);
  }
  return { cols, rows, side: false };
}

function cellLabel(key) {
  const [c, r] = key.split(',').map(Number);
  return c === 5 && isAkp(S.scene) ? `Panel lateral ${r + 1}` : `Fila ${r + 1}, columna ${c + 1}`;
}

function renderDeck(landed) {
  const doc = S.scene, g = geometry(doc);
  const keys = $('#keys'), side = $('#side');
  keys.style.gridTemplateColumns = `repeat(${g.cols}, var(--k))`;
  keys.style.gridTemplateRows = `repeat(${g.rows}, var(--k))`;
  keys.replaceChildren();
  for (let r = 0; r < g.rows; r++) for (let c = 0; c < g.cols; c++) keys.append(cell(`${c},${r}`));
  side.replaceChildren(...(g.side ? [0, 1, 2].map((r) => cell(`5,${r}`)) : []));
  side.hidden = !g.side;
  $('#model').textContent = doc.DeviceUUID || 'Dispositivo sin modelo';
  if (landed) {
    const el = $(`.cell[data-key="${landed}"]`);
    if (el) el.classList.add('landed');
  }
}

function cell(key) {
  const a = (S.scene.Actions || {})[key];
  const el = h('button', {
    type: 'button', class: 'cell' + (a ? '' : ' empty') + (S.sel === key ? ' sel' : ''), 'data-key': key,
    'aria-label': `${cellLabel(key)}: ${a ? a.Name || a.UUID : 'vacía'}`, 'aria-pressed': S.sel === key ? 'true' : 'false',
    draggable: a ? 'true' : null, onclick: () => select(key),
  });
  if (a) {
    const st = stateOf(a);
    el.append(img(cellImg(a), '', a.Name || a.UUID.split('.').pop(), 'fallback'));
    if (st.Title && st.ShowTitle !== false) {
      const span = h('span', { class: 't ' + (st.TitleAlignment || 'bottom') }, st.Title);
      if (st.TitleColor) span.style.color = st.TitleColor;
      const fs = st.FontSize || st.FSize;
      if (fs) span.style.fontSize = `${fs}px`;
      el.append(span);
    }
    el.title = `${a.Name || ''}\n${a.UUID}`;
    el.addEventListener('dragstart', (e) => {
      e.dataTransfer.setData(T_CELL, key);
      e.dataTransfer.effectAllowed = 'copyMove';
      el.classList.add('dragging');
    });
    el.addEventListener('dragend', () => el.classList.remove('dragging'));
  }
  el.addEventListener('dragover', (e) => {
    const t = e.dataTransfer.types;
    if (!t.includes(T_ACTION) && !t.includes(T_CELL)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = t.includes(T_ACTION) || e.ctrlKey || e.altKey ? 'copy' : 'move';
    el.classList.add('over');
  });
  el.addEventListener('dragleave', () => el.classList.remove('over'));
  el.addEventListener('drop', async (e) => {
    e.preventDefault();
    el.classList.remove('over');
    const uuid = e.dataTransfer.getData(T_ACTION), from = e.dataTransfer.getData(T_CELL);
    try {
      if (uuid) await place(key, uuid);
      else if (from && from !== key) {
        const copy = e.ctrlKey || e.altKey;
        await api('POST', `scenes/${S.id}/cells/${from}/move`, { to: key, copy });
        S.sel = key;
        await refresh(key);
        status(copy ? 'Tecla copiada' : 'Tecla movida');
      }
    } catch (err) { fail(err); }
  });
  return el;
}

async function refresh(landed) {
  await openScene(S.id, landed);
  S.scenes = await api('GET', 'scenes');
  renderScenes();
}

async function place(key, uuid) {
  const had = (S.scene.Actions || {})[key];
  await api('PUT', `scenes/${S.id}/cells/${key}`, { UUID: uuid });
  S.sel = key;
  await refresh(key);
  const name = (S.index.get(uuid) || {}).name || uuid;
  status(had ? `«${name}» sustituye a «${had.Name || had.UUID}»` : `«${name}» colocada en ${cellLabel(key).toLowerCase()}`);
}

function select(key) {
  if (S.dirty && S.sel !== key && !confirm('Hay cambios sin guardar en esta tecla. ¿Descartarlos?')) return;
  S.sel = S.sel === key ? null : key;
  for (const c of document.querySelectorAll('.cell')) {
    const on = c.dataset.key === S.sel;
    c.classList.toggle('sel', on);
    c.setAttribute('aria-pressed', on ? 'true' : 'false');
  }
  renderInspector();
}

function updateCellTitle(key, s) {
  if (!key) return;
  const el = $(`.cell[data-key="${key}"]`);
  if (!el) return;
  let t = el.querySelector('.t');
  if (s.Title && s.ShowTitle !== false) {
    if (!t) { t = h('span', { class: 't' }); el.append(t); }
    t.className = 't ' + (s.TitleAlignment || 'bottom');
    t.textContent = s.Title;
    t.style.color = s.TitleColor || '';
    const fs = s.FontSize || s.FSize;
    t.style.fontSize = fs ? `${fs}px` : '';
  } else if (t) {
    t.remove();
  }
}

document.addEventListener('keydown', (e) => {
  if ((e.key === 'Delete' || e.key === 'Backspace') && S.sel && e.target.classList.contains('cell')) removeSel();
  if (e.key === 'Escape' && S.sel && !S.dirty && !/INPUT|TEXTAREA|SELECT/.test(e.target.tagName)) select(S.sel);
});

async function removeSel() {
  const key = S.sel, a = (S.scene.Actions || {})[key];
  if (!a) return;
  try {
    await api('DELETE', `scenes/${S.id}/cells/${key}`);
    S.dirty = false;
    await refresh();
    status(`«${a.Name || a.UUID}» quitada de la tecla`);
  } catch (e) { fail(e); }
}

// ---------- action library ----------
async function loadActions() {
  S.groups = await api('GET', 'actions');
  S.index = new Map();
  for (const g of S.groups) for (const a of g.actions) S.index.set(a.uuid, { ...a, plugin: g.name });
  renderLibrary();
}

function renderLibrary() {
  const term = $('#search').value.trim().toLowerCase();
  const lib = $('#lib');
  lib.replaceChildren();
  for (const g of S.groups) {
    const acts = g.actions.filter((a) => !term || `${a.name} ${a.uuid} ${g.name}`.toLowerCase().includes(term));
    if (!acts.length) continue;
    lib.append(h('details', { class: 'group', open: true },
      h('summary', null, img('/api/image?' + q({ uuid: g.uuid, img: g.icon }), '', ''), h('span', null, g.name), h('span', { class: 'n' }, acts.length)),
      h('ul', { class: 'acts' }, acts.map((a) => {
        const b = h('button', {
          type: 'button', class: 'act', draggable: 'true', title: a.tooltip || a.uuid,
          onclick: () => {
            if (!S.sel) return status('Selecciona primero una tecla, o arrastra la acción hasta ella.');
            place(S.sel, a.uuid).catch(fail);
          },
        }, img('/api/image?' + q({ uuid: a.uuid, img: a.icon }), '', ''), h('span', null, a.name || a.uuid, a.tooltip && h('small', null, a.tooltip)));
        b.addEventListener('dragstart', (e) => { e.dataTransfer.setData(T_ACTION, a.uuid); e.dataTransfer.effectAllowed = 'copy'; });
        return h('li', null, b);
      }))));
  }
  if (!lib.children.length) lib.append(h('p', { class: 'empty-note' }, term ? 'Ninguna acción coincide con la búsqueda.' : 'No hay plugins con acciones disponibles.'));
}
$('#search').addEventListener('input', renderLibrary);

// ---------- inspector ----------
function field(label, input) { return h('label', { class: 'field' }, h('span', null, label), input); }

function renderInspector() {
  const box = $('#inspector');
  S.dirty = false;
  const req = ++S.piReq;
  S.piFrame = null;
  S.piCreds = null;
  const a = S.sel && (S.scene.Actions || {})[S.sel];
  if (!S.sel) return fill(box, sceneInspector());
  if (!a) {
    return box.replaceChildren(h('h2', null, 'Tecla vacía'), h('p', { class: 'sub' }, cellLabel(S.sel)),
      h('p', { class: 'empty-note' }, 'Arrastra una acción hasta aquí, o pulsa una acción de la lista para colocarla en esta tecla.'));
  }
  S.draft = structuredClone(a);
  fill(box, actionInspector());
  mountPi(box, req);
}

// Live plugin inspector (daemon running). Sandboxed WITHOUT allow-same-origin: third-party code gets an
// opaque origin, so it cannot read this editor's cookie, DOM or API. The JSON editor below stays as fallback.
async function mountPi(box, req) {
  const info = S.index.get(S.draft.UUID);
  if (!info?.pi) return;
  try {
    const r = await api('POST', `scenes/${S.id}/cells/${S.sel}/inspector`, {});
    if (req !== S.piReq) return;
    const frame = h('iframe', {
      class: 'pi-frame', sandbox: 'allow-scripts allow-forms', title: 'Inspector del plugin',
      src: r.url, referrerpolicy: 'no-referrer',
    });
    S.piFrame = frame;
    S.piCreds = { port: r.port, token: r.token, info: r.info, actionInfo: r.actionInfo };
    box.insertBefore(h('section', { class: 'pi' }, h('h3', null, 'Inspector del plugin'), frame,
      h('p', { class: 'sub' }, 'El plugin guarda sus ajustes al cambiarlos. Abajo sigue disponible el editor JSON como respaldo.')),
    box.children[2] || null);
  } catch (e) {
    // 501: standalone editor, 404: no live instance of this action. Both simply fall back to the JSON editor.
    if (e.status !== 501 && e.status !== 404) status(e.message, true);
  }
}

window.addEventListener('message', (e) => {
  if (!S.piFrame || e.source !== S.piFrame.contentWindow || !S.piCreds || !e.data || e.data.type !== 'orcadeck-pi-ready') return;
  S.piFrame.contentWindow.postMessage({ type: 'orcadeck-pi-credentials', ...S.piCreds }, '*');
});

function sceneInspector() {
  const doc = S.scene, info = S.scenes.find((s) => s.id === S.id) || {};
  const name = h('input', { type: 'text', value: doc.Name || '' });
  const app = h('input', { type: 'text', value: doc.AppIdentifier0 ?? doc.AppIdentifier ?? '', placeholder: 'C:/ruta/a/la/app.exe', spellcheck: 'false' });
  const save = h('button', { type: 'button', class: 'btn primary', disabled: true }, 'Guardar escena');
  const mark = () => { save.disabled = false; S.dirty = true; };
  name.addEventListener('input', mark);
  app.addEventListener('input', mark);
  save.addEventListener('click', async () => {
    const next = { ...S.scene, Name: name.value.trim() };
    const v = app.value.trim();
    if ('AppIdentifier0' in next || v) { next.AppIdentifier0 = v; next.AppIdentifier = v; }
    try {
      await api('PUT', 'scenes/' + S.id, next);
      await loadScenes();
      await openScene(S.id);
      status('Escena guardada');
    } catch (e) { fail(e); }
  });
  const onDevice = h('input', { type: 'checkbox', checked: !!info.listed });
  onDevice.addEventListener('change', async () => {
    const order = S.scenes.filter((s) => s.listed).map((s) => s.id).filter((x) => x !== S.id);
    if (onDevice.checked) order.push(S.id);
    try { await api('PUT', 'scenes/order', order); await loadScenes(); renderInspector(); status(onDevice.checked ? 'Escena añadida al dispositivo' : 'Escena retirada del dispositivo'); } catch (e) { fail(e); }
  });
  const current = h('button', { type: 'button', class: 'btn', disabled: !!info.current }, info.current ? 'Es la escena actual' : 'Mostrar en el dispositivo');
  current.addEventListener('click', async () => {
    try { await api('PUT', 'scenes/current', { id: S.id }); await loadScenes(); renderInspector(); status('Escena actual cambiada'); } catch (e) { fail(e); }
  });
  return [
    h('h2', null, doc.Name || 'Escena sin nombre'), h('p', { class: 'sub' }, 'Selecciona una tecla para editar su acción.'),
    h('h3', null, 'Escena'),
    field('Nombre', name),
    field('Cambiar a esta escena al usar la app', app),
    h('p', { class: 'sub field-hint' }, 'Ruta del programa. Un asterisco (*) la marca como escena por defecto; vacío, sin cambio automático.'),
    h('label', { class: 'check' }, onDevice, 'Incluida en el dispositivo'),
    h('h3', null, 'Detalles'),
    h('dl', { class: 'kv' },
      h('dt', null, 'Identificador'), h('dd', null, S.id),
      h('dt', null, 'Modelo'), h('dd', null, doc.DeviceUUID || '—'),
      h('dt', null, 'Teclas usadas'), h('dd', null, Object.keys(doc.Actions || {}).length)),
    h('div', { class: 'actions-bar' }, save, current),
  ];
}

function actionInspector() {
  const d = S.draft, info = S.index.get(d.UUID);
  const save = h('button', { type: 'button', class: 'btn primary', disabled: true }, 'Guardar cambios');
  const dirty = () => { S.dirty = true; save.disabled = false; };
  d.States = Array.isArray(d.States) && d.States.length ? d.States : [{}];
  d.Settings = d.Settings && typeof d.Settings === 'object' && !Array.isArray(d.Settings) ? d.Settings : {};
  const st = () => d.States[d.State || 0] || d.States[0];

  // Title and image (per state)
  const stateBox = h('div');
  const drawState = () => {
    const s = st();
    const bind = (input, key, conv = (v) => v) => {
      input.addEventListener('input', () => {
        const v = conv(input);
        if (v === undefined) delete s[key]; else s[key] = v;
        dirty();
        updateCellTitle(S.sel, s);
      });
      return input;
    };
    stateBox.replaceChildren(
      field('Título', bind(h('textarea', { rows: 2, value: s.Title || '', class: 'title-in' }), 'Title', (i) => i.value || undefined)),
      h('label', { class: 'check' }, bind(h('input', { type: 'checkbox', checked: s.ShowTitle !== false }), 'ShowTitle', (i) => (i.checked ? undefined : false)), 'Mostrar título'),
      h('div', { class: 'row2' },
        field('Posición', bind(h('select', null, ['top', 'middle', 'bottom'].map((v) => h('option', { value: v, selected: (s.TitleAlignment || 'bottom') === v }, { top: 'Arriba', middle: 'Centro', bottom: 'Abajo' }[v]))), 'TitleAlignment', (i) => i.value)),
        field('Tamaño', bind(h('input', { type: 'number', min: 6, max: 40, value: s.FontSize ?? '' }), 'FontSize', (i) => (i.value === '' ? undefined : Number(i.value))))),
      field('Color del título', bind(h('input', { type: 'color', value: /^#[0-9a-f]{6}$/i.test(s.TitleColor || '') ? s.TitleColor : '#ffffff' }), 'TitleColor', (i) => i.value)),
      field('Imagen', bind(h('input', { type: 'text', value: s.Image || '', spellcheck: 'false', placeholder: 'Vacío: imagen del plugin' }), 'Image', (i) => i.value)),
    );
  };
  drawState();
  const stateSel = d.States.length > 1 && field('Estado', h('select', {
    onchange: (e) => { d.State = Number(e.target.value); drawState(); dirty(); },
  }, d.States.map((s, i) => h('option', { value: i, selected: (d.State || 0) === i }, s.Name || `Estado ${i + 1}`))));

  // Settings: form for common scalar fields + raw JSON
  const json = h('textarea', { spellcheck: 'false', 'aria-label': 'Ajustes en JSON', value: JSON.stringify(d.Settings, null, 2) });
  const jsonErr = h('p', { class: 'err-msg', hidden: true });
  const form = h('div');
  const syncJson = () => { json.value = JSON.stringify(d.Settings, null, 2); json.classList.remove('bad'); jsonErr.hidden = true; };
  const drawForm = () => {
    const rows = [];
    if (d.UUID === ROTATE) {
      rows.push(field('Ir a la escena', h('select', {
        onchange: (e) => { d.Settings.ProfileUUID = e.target.value; d.Settings.DeviceUUID ??= ''; syncJson(); dirty(); },
      }, h('option', { value: '', selected: !d.Settings.ProfileUUID }, 'Siguiente escena'),
      S.scenes.map((s) => h('option', { value: s.id, selected: d.Settings.ProfileUUID === s.id }, s.name || s.id)))));
    }
    for (const [k, v] of Object.entries(d.Settings)) {
      if (d.UUID === ROTATE && (k === 'ProfileUUID' || k === 'DeviceUUID')) continue;
      if (typeof v === 'boolean') {
        rows.push(h('label', { class: 'check' }, h('input', { type: 'checkbox', checked: v, onchange: (e) => { d.Settings[k] = e.target.checked; syncJson(); dirty(); } }), k));
      } else if (typeof v === 'string' || typeof v === 'number') {
        rows.push(field(k, h('input', {
          type: typeof v === 'number' ? 'number' : 'text', value: String(v), spellcheck: 'false',
          oninput: (e) => { d.Settings[k] = typeof v === 'number' ? Number(e.target.value) : e.target.value; syncJson(); dirty(); },
        })));
      }
    }
    form.replaceChildren(...(rows.length ? rows : [h('p', { class: 'empty-note' }, 'Esta acción no tiene ajustes sencillos. Edítalos en JSON si el plugin los usa.')]));
  };
  drawForm();
  json.addEventListener('input', () => {
    try {
      const v = JSON.parse(json.value || '{}');
      if (!v || typeof v !== 'object' || Array.isArray(v)) throw new Error('Los ajustes deben ser un objeto JSON { … }');
      d.Settings = v; json.classList.remove('bad'); jsonErr.hidden = true; dirty();
    } catch (e) { json.classList.add('bad'); jsonErr.hidden = false; jsonErr.textContent = e.message; save.disabled = true; }
  });
  json.addEventListener('change', () => { if (!json.classList.contains('bad')) drawForm(); });

  save.addEventListener('click', async () => {
    try {
      await api('PUT', `scenes/${S.id}/cells/${S.sel}`, d);
      S.dirty = false;
      await refresh(S.sel);
      status('Cambios guardados');
    } catch (e) { fail(e); }
  });

  const preview = h('div', { class: 'preview' }, img(cellImg(d), '', ''));
  return [
    h('div', { class: 'insp-head' }, preview, h('div', null, h('h2', null, d.Name || info?.name || 'Acción'), h('p', { class: 'sub' }, info ? info.plugin : 'Plugin no instalado'))),
    h('p', { class: 'sub' }, `${cellLabel(S.sel)} · ${d.UUID}`),
    h('h3', null, 'Aspecto'), stateSel, stateBox,
    h('h3', null, 'Ajustes'), form,
    h('details', null, h('summary', null, 'Ajustes en JSON'), json, jsonErr),
    h('div', { class: 'actions-bar' }, save, h('button', { type: 'button', class: 'btn danger', onclick: removeSel }, 'Quitar de la tecla')),
  ];
}

// ---------- plugins view ----------
const RUNTIME = { exe: 'Programa nativo', node: 'Node.js', html: 'Página web', builtin: 'Integrado', other: 'Otro', unknown: 'Desconocido' };
const nActions = (n) => `${n} ${n === 1 ? 'acción' : 'acciones'}`;
const STATUS = { ok: ['Lista', 'ok'], 'needs-manifest': ['Falta manifest', 'warn'], invalid: ['Manifest ilegible', 'bad'] };

async function loadPlugins() {
  S.plugins = await api('GET', 'plugins');
  renderPlugins();
}

function renderPlugins() {
  const only = $('#only-problems').checked;
  const list = S.plugins.filter((p) => !only || p.status !== 'ok').sort((a, b) => a.name.localeCompare(b.name));
  $('#plist').replaceChildren(...list.map((p) => {
    const [label, cls] = STATUS[p.status] || [p.status, ''];
    return h('li', null, h('button', {
      type: 'button', 'aria-current': S.psel === p.uuid ? 'true' : 'false',
      onclick: () => { S.psel = p.uuid; renderPlugins(); renderPluginDetail(p); },
    }, img('/api/image?' + q({ uuid: p.uuid, img: p.icon }), '', ''),
    h('span', null, h('strong', null, p.name), h('span', { class: 'meta' }, ` ${p.version ? 'v' + p.version : ''} ${p.author ? 'de ' + p.author : ''}`)),
    h('span', { class: 'meta' }, p.status === 'ok' ? `${nActions(p.actions)}, ${RUNTIME[p.runtime] || p.runtime}` : ''),
    h('span', { class: 'pill ' + cls }, label)));
  }));
  if (!list.length) $('#plist').append(h('li', null, h('p', { class: 'empty-note' }, only ? 'Ningún plugin tiene problemas.' : 'No hay plugins instalados.')));
}
$('#only-problems').addEventListener('change', renderPlugins);

function renderPluginDetail(p) {
  const box = $('#pdetail');
  const note = {
    'needs-manifest': 'Su manifest viene cifrado por Elgato y aún no hay uno reconstruido. Reconstrúyelo con tools/plugin-install o aporta el manifest del fabricante; hasta entonces sus acciones no aparecen.',
    invalid: 'El manifest no es JSON válido. El plugin no se puede cargar hasta corregirlo o reinstalarlo.',
  }[p.status];
  const origin = p.origin === 'orcadeck' ? 'Instalado por OrcaDeck'
    : p.builtin ? (p.uuid === 'com.miguelpicado.orcadock' ? 'Integrado en OrcaDeck' : 'Integrado en Stream Dock (solo lectura)')
    : 'Instalado (Stream Dock, solo lectura)';
  const pre = h('pre', { class: 'json', hidden: true });
  const show = h('button', { type: 'button', class: 'btn' }, 'Ver manifest');
  show.addEventListener('click', async () => {
    try { pre.textContent = JSON.stringify(await api('GET', `plugins/${encodeURIComponent(p.uuid)}/manifest`), null, 2); pre.hidden = false; show.remove(); } catch (e) { fail(e); }
  });
  fill(box, [
    h('div', { class: 'insp-head' }, h('div', { class: 'preview' }, img('/api/image?' + q({ uuid: p.uuid, img: p.icon }), '', '')),
      h('div', null, h('h2', null, p.name), h('p', { class: 'sub' }, p.uuid))),
    p.description && h('p', null, p.description),
    note && h('p', { class: 'note' }, note),
    h('dl', { class: 'kv' },
      h('dt', null, 'Estado'), h('dd', null, (STATUS[p.status] || [p.status])[0]),
      h('dt', null, 'Versión'), h('dd', null, p.version || '—'),
      h('dt', null, 'Autor'), h('dd', null, p.author || '—'),
      h('dt', null, 'Se ejecuta como'), h('dd', null, RUNTIME[p.runtime] || p.runtime),
      h('dt', null, 'Acciones'), h('dd', null, p.actions),
      h('dt', null, 'Manifest'), h('dd', null, p.overlay ? 'Reconstruido (overlay)' : p.encrypted ? 'Cifrado' : 'Original'),
      h('dt', null, 'Origen'), h('dd', null, origin),
      h('dt', null, 'Carpeta'), h('dd', null, p.dir)),
    h('div', { class: 'actions-bar' },
      p.status !== 'needs-manifest' && show,
      p.removable && h('button', { type: 'button', class: 'btn danger', onclick: () => removePlugin(p) }, 'Quitar plugin')),
    pre,
  ]);
}

async function removePlugin(p) {
  if (!confirm(`¿Quitar «${p.name || p.uuid}»?\n\nSe borrará ${p.dir}. Las teclas que usen sus acciones dejarán de funcionar.`)) return;
  try {
    await api('DELETE', 'plugins/' + encodeURIComponent(p.uuid));
    S.psel = null;
    $('#pdetail').replaceChildren(h('p', { class: 'empty-note' }, 'Selecciona un plugin de la lista.'));
    await loadPlugins();
    await loadActions();
    status(`«${p.name || p.uuid}» quitado`);
  } catch (e) { fail(e); }
}

// ---------- install from Downloads (on demand, nothing watches the folder) ----------
$('#scan-downloads').addEventListener('click', async function () {
  this.disabled = true;
  try {
    renderInbox(await api('GET', 'plugins/inbox'));
  } catch (e) { fail(e); } finally { this.disabled = false; }
});

function renderInbox({ dir, items }) {
  const note = $('#inbox-note'), list = $('#inbox');
  note.hidden = false;
  note.textContent = items.length
    ? `${items.length} paquete${items.length === 1 ? '' : 's'} reciente${items.length === 1 ? '' : 's'} en ${dir}. Nada se instala hasta que lo confirmes.`
    : `No hay paquetes recientes en ${dir}.`;
  list.hidden = items.length === 0;
  list.replaceChildren(...items.map((it) => h('li', null,
    h('button', { type: 'button', onclick: () => inspectPackage(it.path) },
      h('span', null, h('strong', null, it.name),
        h('span', { class: 'meta' }, ` · ${it.kind === 'folder' ? 'carpeta' : 'archivo'} · ${new Date(it.modified).toLocaleString()}`)),
      h('span', { class: 'meta' }, 'Revisar')))));
}

async function inspectPackage(path) {
  try {
    const p = await api('POST', 'plugins/inspect', { path });
    S.psel = null;
    if (S.plugins) renderPlugins();
    renderPackage(p);
  } catch (e) { fail(e); }
}

function renderPackage(p) {
  const box = $('#pdetail');
  const staged = !!p.token;
  const install = h('button', { type: 'button', class: 'btn primary' }, p.installed ? 'Reemplazar plugin' : 'Instalar plugin');
  install.addEventListener('click', async () => {
    const ask = p.installed
      ? `Ya hay un «${p.uuid}» instalado. ¿Reemplazarlo? Se guardará una copia de seguridad.`
      : `¿Instalar «${p.name || p.uuid}»?`;
    if (!confirm(ask)) return;
    install.disabled = true;
    try {
      const r = await api('POST', 'plugins/install', staged ? { staged: p.token, force: p.installed } : { path: p.path, force: p.installed });
      if (staged) history.replaceState(null, '', '/');
      await loadPlugins();
      await loadActions();
      const found = (S.plugins || []).find((x) => x.uuid === r.uuid);
      if (found) { S.psel = found.uuid; renderPlugins(); renderPluginDetail(found); }
      status(`«${p.name || r.uuid}» ${r.replaced ? 'reemplazado' : 'instalado'}${r.needsManifest ? ' (necesita un manifest)' : ''}. Arrastra sus acciones a una tecla en la vista Teclas.`);
    } catch (e) { fail(e); install.disabled = false; }
  });
  fill(box, [
    h('h2', null, 'Instalar plugin'),
    h('p', { class: 'sub' }, staged ? p.file : p.path),
    h('h3', null, p.name || p.uuid),
    p.description && h('p', null, p.description),
    h('dl', { class: 'kv' },
      h('dt', null, 'UUID'), h('dd', null, p.uuid),
      h('dt', null, 'Versión'), h('dd', null, p.version || '—'),
      h('dt', null, 'Autor'), h('dd', null, p.author || '—'),
      h('dt', null, 'Acciones'), h('dd', null, p.actions),
      h('dt', null, 'Contenido'), h('dd', null, p.kind === 'folder' ? 'Carpeta .sdPlugin' : 'Archivo comprimido'),
      h('dt', null, 'Manifest'), h('dd', null, p.encrypted ? `Cifrado (${p.confidence})` : 'Original'),
      h('dt', null, 'Destino'), h('dd', null, p.installed ? 'Ya instalado: se reemplazará' : 'No instalado')),
    p.needsManifest && h('p', { class: 'note' }, 'El manifest viene cifrado y no se pudo reconstruir entero: el plugin se instalará, pero no cargará hasta aportar un manifest del fabricante.'),
    h('div', { class: 'actions-bar' }, install,
      h('button', {
        type: 'button', class: 'btn',
        onclick: () => { S.psel = null; if (S.plugins) renderPlugins(); box.replaceChildren(h('p', { class: 'empty-note' }, 'Selecciona un plugin de la lista.')); },
      }, 'Cancelar')),
  ]);
}

// ---------- chrome: views, theme, device ----------
function showView(v) {
  for (const o of document.querySelectorAll('.views button')) o.setAttribute('aria-pressed', o.dataset.view === v ? 'true' : 'false');
  $('#keys-view').hidden = v !== 'keys';
  $('#plugins-view').hidden = v !== 'plugins';
  if (v === 'plugins' && !S.plugins) loadPlugins().catch(fail);
}
for (const b of document.querySelectorAll('.views button')) {
  b.addEventListener('click', () => showView(b.dataset.view));
}

// Package opened from the OS shell (`orcadeck install`, double click): the URL carries a one-use
// token, never a path. The page only shows what will be installed; nothing happens without confirm.
async function openStaged(token) {
  try {
    const p = await api('GET', 'plugins/staged?token=' + encodeURIComponent(token));
    S.psel = null;
    if (S.plugins) renderPlugins();
    renderPackage(p);
  } catch (e) { fail(e); }
}
const stagedToken = new URLSearchParams(location.search).get('install');

const store = { get: (k) => { try { return localStorage.getItem(k); } catch { return null; } }, set: (k, v) => { try { localStorage.setItem(k, v); } catch { /* private mode */ } } };
const savedTheme = store.get('orcadeck-theme');
if (savedTheme) document.documentElement.dataset.theme = savedTheme;
$('#theme').addEventListener('click', () => {
  const dark = document.documentElement.dataset.theme ? document.documentElement.dataset.theme === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
  document.documentElement.dataset.theme = dark ? 'light' : 'dark';
  store.set('orcadeck-theme', document.documentElement.dataset.theme);
});

function renderDevice(d) {
  $('#bright').value = d.brightness;
  $('#bright-val').textContent = d.brightness;
  $('#dev-status').classList.toggle('on', !!d.connected);
  $('#dev-status').title = d.connected ? `${d.model} conectado` : `${d.model} sin conexión${d.stub ? ' (simulado)' : ''}`;
}
$('#bright').addEventListener('input', (e) => { $('#bright-val').textContent = e.target.value; });
$('#bright').addEventListener('change', (e) => api('PUT', 'device', { brightness: Number(e.target.value) }).then(renderDevice).catch(fail));

window.addEventListener('beforeunload', (e) => { if (S.dirty) e.preventDefault(); });

(async function boot() {
  try {
    const [, , device] = await Promise.all([loadScenes(), loadActions(), api('GET', 'device')]);
    renderDevice(device);
    const first = S.scenes.find((s) => s.current) || S.scenes.find((s) => s.listed) || S.scenes[0];
    if (first) await openScene(first.id);
    else status('No hay escenas. Crea una con «Nueva escena».');
    if (stagedToken) { showView('plugins'); await openStaged(stagedToken); }
  } catch (e) { fail(e); }
})();
