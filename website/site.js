// Theme switch, tabs, live views and the typing terminal. No cookies, no requests to other sites. The theme choice
// stays in this browser (see theme.js).
//
// Live views: each .live box holds the screenshot (poster). On screen, unless reduced motion is on or the screen
// is small (the hero runs on small screens too), it loads the app's own interface from /app/ in demo mode in a
// frame from this site, runs a few setup steps in it (search, select a topic...), then shows it scaled over the
// poster, without interaction except the hero (see guard). Off screen it is removed again; its pause button
// brings the poster back.
(function () {
  var root = document.documentElement;
  root.classList.add('js');
  var reduce = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)');
  var sysDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)');
  document.addEventListener('click', function (ev) {
    var a = ev.target.closest && ev.target.closest('a[aria-disabled="true"]');
    if (a) ev.preventDefault();
  });

  // ---- theme: light or dark; until a choice is made the system one is shown as selected
  function theme() {
    return root.getAttribute('data-theme') || (sysDark && sysDark.matches ? 'dark' : 'light');
  }
  var themeBtns = Array.prototype.slice.call(document.querySelectorAll('[data-theme-set]'));
  function showTheme() {
    themeBtns.forEach(function (b) { b.setAttribute('aria-pressed', b.getAttribute('data-theme-set') === theme() ? 'true' : 'false'); });
  }
  themeBtns.forEach(function (b) {
    b.addEventListener('click', function () {
      var v = b.getAttribute('data-theme-set');
      if (v === theme()) return;
      root.setAttribute('data-theme', v);
      try { localStorage.setItem('theme', v); } catch (e) {}
      showTheme();
      lives.forEach(function (l) { if (l.frame) { unmount(l); mount(l); } });
    });
  });
  if (sysDark && sysDark.addEventListener) sysDark.addEventListener('change', function () {
    showTheme();
    if (!root.getAttribute('data-theme')) lives.forEach(function (l) { if (l.frame) { unmount(l); mount(l); } });
  });
  showTheme();

  // ---- tabs: click or arrow keys switch the panel
  Array.prototype.forEach.call(document.querySelectorAll('.looks'), function (box) {
    var tabs = Array.prototype.slice.call(box.querySelectorAll('[role=tab]'));
    function select(i, focus) {
      tabs.forEach(function (t, j) {
        var on = i === j;
        t.setAttribute('aria-selected', on ? 'true' : 'false');
        t.tabIndex = on ? 0 : -1;
        var panel = document.getElementById(t.getAttribute('aria-controls'));
        if (on) panel.removeAttribute('data-off'); else panel.setAttribute('data-off', '');
      });
      if (focus) tabs[i].focus();
    }
    tabs.forEach(function (t, i) {
      t.addEventListener('click', function () { select(i, false); });
      t.addEventListener('keydown', function (ev) {
        var n = tabs.length, k = ev.key;
        if (k === 'ArrowRight') select((i + 1) % n, true);
        else if (k === 'ArrowLeft') select((i + n - 1) % n, true);
        else if (k === 'Home') select(0, true);
        else if (k === 'End') select(n - 1, true);
        else return;
        ev.preventDefault();
      });
    });
  });

  // ---- live views
  var W = 1280, H = 800;
  function sleep(ms) { return new Promise(function (r) { setTimeout(r, ms); }); }
  function find(doc, sel, text, n) {
    var all = Array.prototype.slice.call(doc.querySelectorAll(sel));
    if (text != null) all = all.filter(function (el) { return el.textContent.indexOf(text) >= 0; });
    return all[n || 0] || null;
  }
  async function waitFor(doc, sel, text, n) {
    for (var i = 0; i < 80; i++) {
      var el = find(doc, sel, text, n);
      if (el) return el;
      await sleep(100);
    }
    throw new Error('not found: ' + sel);
  }
  function setValue(win, el, v, proto, evt) {
    Object.getOwnPropertyDescriptor(win[proto].prototype, 'value').set.call(el, v);
    el.dispatchEvent(new win.Event(evt, { bubbles: true }));
  }
  async function run(l, win) {
    var doc = win.document;
    await waitFor(doc, '#app-root');
    await sleep(500);
    for (var i = 0; i < l.steps.length; i++) {
      if (l.gen !== l.runId) return;
      var s = l.steps[i], el;
      if (s[0] === 'wait') { await sleep(s[1]); continue; }
      if (s[0] === 'fill') { el = await waitFor(doc, s[1]); setValue(win, el, s[2], 'HTMLInputElement', 'input'); }
      else if (s[0] === 'select') { el = await waitFor(doc, s[1]); setValue(win, el, s[2], 'HTMLSelectElement', 'change'); }
      else if (s[0] === 'click') { (await waitFor(doc, s[1])).click(); }
      else if (s[0] === 'clickNth') { (await waitFor(doc, s[1], null, s[2])).click(); }
      else if (s[0] === 'clickText') { (await waitFor(doc, s[1], s[2], s[3])).click(); }
      await sleep(250);
    }
    await sleep(400);
  }
  // The interactive demo (the hero): the left menu and the connection tabs answer, nothing else does (owner,
  // 18 Oct 2026). The wheel scrolls a list while it can, then the page; a page scroll already under way keeps
  // scrolling the page when the pointer passes over the demo.
  var ALLOW = '[data-testid^="rail-"]:not([data-testid="rail-settings"]), [data-testid="cbar-tab"]';
  var BLOCK = '[data-testid="cbar-close"]';
  var lastPage = 0;
  window.addEventListener('wheel', function () { lastPage = Date.now(); }, { passive: true });
  function guard(win) {
    var doc = win.document;
    var css = doc.createElement('style');
    css.textContent = '*{cursor:default!important}' + ALLOW + '{cursor:pointer!important}' + BLOCK + '{cursor:default!important}';
    doc.head.appendChild(css);
    function ok(t) { return t && t.closest && t.closest(ALLOW) && !t.closest(BLOCK); }
    ['pointerdown', 'mousedown', 'mouseup', 'click', 'dblclick', 'auxclick', 'contextmenu'].forEach(function (type) {
      doc.addEventListener(type, function (ev) {
        if (ok(ev.target)) return;
        ev.preventDefault(); ev.stopImmediatePropagation();
        if (type === 'pointerdown' && doc.activeElement && doc.activeElement.blur) doc.activeElement.blur();
      }, true);
    });
    ['keydown', 'keypress', 'keyup'].forEach(function (type) {
      doc.addEventListener(type, function (ev) { ev.preventDefault(); ev.stopImmediatePropagation(); }, true);
    });
    function scroller(el, dy) {
      for (; el && el !== doc.documentElement; el = el.parentElement) {
        var oy = win.getComputedStyle(el).overflowY;
        if ((oy === 'auto' || oy === 'scroll') && el.scrollHeight > el.clientHeight + 1) {
          if (dy > 0 ? el.scrollTop + el.clientHeight < el.scrollHeight - 1 : el.scrollTop > 0) return el;
        }
      }
      return null;
    }
    doc.addEventListener('wheel', function (ev) {
      var dy = ev.deltaY * (ev.deltaMode === 1 ? 16 : ev.deltaMode === 2 ? win.innerHeight : 1);
      if (Math.abs(ev.deltaX) > Math.abs(dy)) return;
      var now = Date.now();
      var inner = now - lastPage > 400 ? scroller(ev.target, dy) : null;
      if (inner) return;
      ev.preventDefault();
      lastPage = now;
      window.scrollBy(0, dy);
    }, { passive: false, capture: true });
  }
  function fit(l) {
    if (!l.frame) return;
    var k = l.box.clientWidth / W;
    l.frame.style.transform = 'scale(' + k + ')';
  }
  function allowed(l) {
    if (l.paused || (reduce && reduce.matches)) return false;
    return l.hero || window.innerWidth >= 700;
  }
  function mount(l) {
    if (l.frame || !allowed(l)) return;
    var f = document.createElement('iframe');
    f.src = '/app/?' + l.query + '&theme=' + theme();
    f.width = W; f.height = H;
    f.tabIndex = -1;
    f.setAttribute('aria-hidden', 'true');
    f.setAttribute('title', '');
    f.className = 'lframe';
    l.frame = f;
    l.runId = (l.gen = (l.gen || 0) + 1);
    var id = l.runId;
    f.addEventListener('load', function () {
      run(l, f.contentWindow).then(function () {
        if (l.frame !== f || l.runId !== id) return;
        if (l.interactive && window.innerWidth >= 700) { guard(f.contentWindow); f.classList.add('interactive'); }
        f.classList.add('ready');
      }).catch(function () { if (l.frame === f) unmount(l); });
    });
    l.box.appendChild(f);
    fit(l);
    l.btn.hidden = false;
  }
  function unmount(l) {
    if (!l.frame) return;
    l.gen++;
    l.frame.remove();
    l.frame = null;
  }
  function syncBtn(l) {
    var label = l.btn.getAttribute(l.paused ? 'data-play' : 'data-pause');
    l.btn.setAttribute('aria-label', label); l.btn.title = label;
    l.btn.firstElementChild.textContent = l.paused ? '▶' : '❚❚';
    l.btn.hidden = (reduce && reduce.matches) || (!l.hero && window.innerWidth < 700);
  }
  var lives = Array.prototype.slice.call(document.querySelectorAll('.live')).map(function (box) {
    var l = { box: box, query: box.getAttribute('data-query'), steps: JSON.parse(box.getAttribute('data-steps')),
      hero: box.classList.contains('hero-live'), interactive: box.hasAttribute('data-interactive'), btn: box.querySelector('.lpause'), frame: null, paused: false, seen: false };
    l.btn.addEventListener('click', function () {
      l.paused = !l.paused;
      if (l.paused) unmount(l); else if (l.seen) mount(l);
      syncBtn(l);
    });
    syncBtn(l);
    return l;
  });
  if ('IntersectionObserver' in window) {
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (en) {
        var l = lives.filter(function (x) { return x.box === en.target; })[0];
        l.seen = en.isIntersecting;
        if (en.isIntersecting) mount(l); else unmount(l);
      });
    }, { rootMargin: '200px 0px' });
    lives.forEach(function (l) { io.observe(l.box); });
  }
  window.addEventListener('resize', function () {
    lives.forEach(function (l) { fit(l); syncBtn(l); if (!allowed(l)) unmount(l); else if (l.seen) mount(l); });
  });
  if (reduce && reduce.addEventListener) reduce.addEventListener('change', function () {
    lives.forEach(function (l) { syncBtn(l); if (!allowed(l)) unmount(l); else if (l.seen) mount(l); });
  });

  // ---- terminal: the commands type themselves, one after the other, then start again
  Array.prototype.forEach.call(document.querySelectorAll('.term[data-type]'), function (term) {
    if (reduce && reduce.matches) return;
    var lines = Array.prototype.slice.call(term.querySelectorAll('.cmd'));
    var full = lines.map(function (c) { return c.textContent; });
    var on = false, timer = null;
    function play() {
      lines.forEach(function (c) { c.textContent = ''; c.parentNode.classList.remove('done'); });
      var li = 0, ci = 0;
      (function step() {
        if (!on) return;
        if (li >= lines.length) { timer = setTimeout(play, 3500); return; }
        ci++;
        lines[li].textContent = full[li].slice(0, ci);
        if (ci >= full[li].length) { lines[li].parentNode.classList.add('done'); li++; ci = 0; timer = setTimeout(step, 900); }
        else timer = setTimeout(step, 38);
      })();
    }
    new IntersectionObserver(function (en) {
      var vis = en[0].isIntersecting;
      if (vis && !on) { on = true; play(); }
      if (!vis && on) { on = false; clearTimeout(timer); lines.forEach(function (c, i) { c.textContent = full[i]; }); }
    }).observe(term);
  });
})();
