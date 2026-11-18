;; Palette slots by position (the default look's palette; other looks tone
;; these slots), the chain's hinges, and a glowing pair grouped for glow.scm.
(define (paint x y r g b) (set-color (body-index-at x y) r g b))
(paint -4.6 -2.0 0.740 0.348 0.327) ; red
(paint -4.6 -1.1 0.610 0.459 0.029) ; amber
(paint -4.6 -0.4 0.335 0.471 0.787) ; blue
(paint -2.6 -2.1 0.024 0.558 0.537) ; teal
(paint -2.9 -1.2 0.710 0.390 0.127) ; orange
(paint -0.6 2.2 0.534 0.407 0.740)  ; violet
(paint -0.6 1.25 0.011 0.531 0.702) ; sky
(paint -0.6 0.3 0.692 0.349 0.554)  ; pink
(paint 1.4 -1.9 0.211 0.567 0.315)  ; green
(paint 2.5 -2.15 0.610 0.459 0.029) ; amber (glows)
(paint 3.9 -1.6 0.417 0.535 0.141)  ; lime
(paint 4.9 -2.2 0.740 0.348 0.327)  ; red (glows)
(hinge (body-index-at -0.6 2.2) -1 -0.6 2.6)
(hinge (body-index-at -0.6 2.2) (body-index-at -0.6 1.25) -0.6 1.7)
(hinge (body-index-at -0.6 1.25) (body-index-at -0.6 0.3) -0.6 0.75)
(group (list (body-index-at 2.5 -2.15) (body-index-at 4.9 -2.2)))
