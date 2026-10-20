// mqtt-panel.com: redirects www. to the apex, serves the static assets, adds security headers.
// The pages embed the app's own interface from /app/ (demo mode, mock data, no network) in frames from the same
// site: frames are allowed from 'self' only, and nobody else may frame the site.
const CSP =
  "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; media-src 'self'; frame-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'self'";
// The app interface draws a few images from data: and blob: URLs (decoded payloads, icons), and its code editor
// (Publish screen) adds its own <style> element, so inline styles are allowed under /app/ only. It runs demo data
// only and shows nothing from other sites; the pages themselves keep style-src 'self'.
const CSP_APP =
  "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'self'";
const HEADERS = {
  'Referrer-Policy': 'no-referrer',
  'X-Content-Type-Options': 'nosniff',
  'Permissions-Policy': 'camera=(), microphone=(), geolocation=()',
  'Strict-Transport-Security': 'max-age=31536000',
};

// Preview protection (owner, 18 Oct 2026: the app is not released yet). While the Worker secret SITE_PASSWORD is
// set, every request needs HTTP Basic authentication with the user name in SITE_USER (default "preview") and that
// password, and search engines are told not to index. Remove the secret to make the site public again:
//   wrangler secret delete SITE_PASSWORD
const enc = new TextEncoder();
function same(a, b) {
  const x = enc.encode(a), y = enc.encode(b);
  if (x.length !== y.length) return false;
  return crypto.subtle.timingSafeEqual(x, y);
}
function authorised(request, env) {
  const h = request.headers.get('Authorization') || '';
  if (!h.startsWith('Basic ')) return false;
  let pair = '';
  try { pair = atob(h.slice(6)); } catch (e) { return false; }
  const i = pair.indexOf(':');
  if (i < 0) return false;
  const user = same(pair.slice(0, i), env.SITE_USER || 'preview');
  const pass = same(pair.slice(i + 1), env.SITE_PASSWORD);
  return user && pass; // both compared, so the time does not tell which part was wrong
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.hostname.startsWith('www.')) {
      url.hostname = url.hostname.slice(4);
      return Response.redirect(url.toString(), 301);
    }
    const locked = !!env.SITE_PASSWORD;
    if (locked && !authorised(request, env)) {
      return new Response('This site is not public yet.\n', {
        status: 401,
        headers: {
          'WWW-Authenticate': 'Basic realm="mqtt-panel.com preview", charset="UTF-8"',
          'Content-Type': 'text/plain; charset=utf-8',
          'X-Robots-Tag': 'noindex, nofollow',
          'Cache-Control': 'no-store',
          ...HEADERS,
        },
      });
    }
    const res = await env.ASSETS.fetch(request);
    const out = new Response(res.body, res);
    if (locked) {
      out.headers.set('X-Robots-Tag', 'noindex, nofollow');
      out.headers.set('Cache-Control', 'private, no-store');
    }
    for (const [k, v] of Object.entries(HEADERS)) out.headers.set(k, v);
    out.headers.set('Content-Security-Policy', url.pathname.startsWith('/app/') ? CSP_APP : CSP);
    return out;
  },
};
