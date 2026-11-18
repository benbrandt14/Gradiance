;; `spring-feel` option "calm": hooks made static, the right-hand box made
;; dense, and each body hung on a spring (frequency Hz, damping ratio).
(set-static (body-index-at -3 2) 1)
(set-static (body-index-at 0 2) 1)
(set-static (body-index-at 3 2) 1)
(set-density (body-index-at 3 0.5) 10)
(spring (body-index-at -3 2) (body-index-at -3 0.5) 1.0 0.3)
(spring (body-index-at 0 2) (body-index-at 0 0.5) 1.0 0.3)
(spring (body-index-at 3 2) (body-index-at 3 0.5) 1.0 0.3)
