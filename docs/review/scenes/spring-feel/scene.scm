;; Shared by every `spring-feel` option: three hooks and three bodies of very
;; different mass -- a 0.15 m pebble, a 1 m box, and a 2 m box that the
;; option scripts make ten times denser (40 kg). Options differ only in the
;; springs they hang the bodies on.
(spawn-box -3 2 0.3 0.1)
(spawn-box 0 2 0.3 0.1)
(spawn-box 3 2 0.3 0.1)
(spawn-box -3 0.5 0.15 0.15)
(spawn-box 0 0.5 1 1)
(spawn-box 3 0.5 2 2)
