;; Turns the pool and the tank into water (density 2 kg/m2, drag 2 /s) and
;; makes the tank's ball heavy, so it sinks while the others float.
(set-fluid (body-index-at -1.2 -1.7) 2 2)
(set-fluid (body-index-at 4 -1.7) 2 2)
(set-density (body-index-at 4 1.5) 6)
;; Water colours (the palette would roll a random hue): a blue pool and a
;; teal tank, so the options are judged on shading, not on the hue.
(set-color (body-index-at -1.2 -1.7) 0.22 0.52 0.86)
(set-color (body-index-at 4 -1.7) 0.16 0.62 0.66)
