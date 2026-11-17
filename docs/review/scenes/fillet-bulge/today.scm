;; Option A of `fillet-bulge`: today's fillet everywhere. The flush pair's
;; top and bottom faces bulge at the seam (up to radius/4 = 0.1 m).
(set-static (body-index-at 0 -2) 1)
(fillet-merge (body-index-at -1.5 -1.3) (body-index-at -0.5 -1.3) 0.4)
(fillet-merge (body-index-at 1.5 -1.6) (body-index-at 1.5 -0.9) 0.4)
