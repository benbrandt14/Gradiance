;; Cuts polygon.scm's box across its top-left corner into a pentagon and a
;; triangle (polygon shapes, not boxes). The colour is set first: a run
;; reads one snapshot, so the pieces inherit it from the box.
(set-color (body-index-at 2.6 1.3) 0.335 0.471 0.787) ; blue
(cut 1.6 0.9 3.0 2.1 0.06)
