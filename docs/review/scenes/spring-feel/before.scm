;; `spring-feel` option "before": the pre-4b-4 default, k = 100 N/m per m^2
;; of the body (density ignored), no damping. Hooks made static, the right box
;; dense, and each body hung on a spring (frequency Hz, damping ratio).
(set-static (body-index-at -3 2) 1)
(set-static (body-index-at 0 2) 1)
(set-static (body-index-at 3 2) 1)
(set-density (body-index-at 3 0.5) 10)
(spring (body-index-at -3 2) (body-index-at -3 0.5) 1.59 0)
(spring (body-index-at 0 2) (body-index-at 0 0.5) 1.59 0)
(spring (body-index-at 3 2) (body-index-at 3 0.5) 0.5 0)
