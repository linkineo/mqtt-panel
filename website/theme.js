// Applies a light or dark choice made with the switch in the header, before the page paints. Without a choice the
// page follows the system. The choice stays in this browser only (localStorage), never sent anywhere.
(function () {
  try {
    var t = localStorage.getItem('theme');
    if (t === 'light' || t === 'dark') document.documentElement.setAttribute('data-theme', t);
  } catch (e) {}
})();
