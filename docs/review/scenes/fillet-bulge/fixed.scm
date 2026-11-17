;; Option B of `fillet-bulge`: what an exact fillet would give -- flush
;; faces stay flat (shown with a plain merge), the T's inside corner keeps
;; its fillet.
(set-static (body-index-at 0 -2) 1)
(merge (body-index-at -1.5 -1.3) (body-index-at -0.5 -1.3))
(fillet-merge (body-index-at 1.5 -1.6) (body-index-at 1.5 -0.9) 0.4)
