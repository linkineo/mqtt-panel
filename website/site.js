// Plays the demo clips while they are on screen, unless the visitor prefers reduced motion (the poster then stays).
// Disabled store buttons do nothing when clicked. No cookies, no storage, no requests beyond the clips.
(function () {
  document.addEventListener('click', function (ev) {
    var a = ev.target.closest && ev.target.closest('a[aria-disabled="true"]');
    if (a) ev.preventDefault();
  });
  var reduce = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)');
  var videos = Array.prototype.slice.call(document.querySelectorAll('video'));
  if (!videos.length) return;
  function stopAll() {
    videos.forEach(function (v) { v.pause(); });
  }
  if (!('IntersectionObserver' in window)) return;
  var io = new IntersectionObserver(function (entries) {
    entries.forEach(function (en) {
      var v = en.target;
      if (reduce && reduce.matches) return;
      if (en.isIntersecting) {
        var p = v.play();
        if (p && p.catch) p.catch(function () {});
      } else {
        v.pause();
      }
    });
  }, { threshold: 0.35 });
  videos.forEach(function (v) { io.observe(v); });
  if (reduce && reduce.addEventListener) reduce.addEventListener('change', function () { if (reduce.matches) stopAll(); });
})();
