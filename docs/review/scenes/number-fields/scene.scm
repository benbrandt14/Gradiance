;; Shared by every `number-fields` option: a ground and a 1 m box on a hinge,
;; with Properties and Settings open, so there are length, angle, ratio and
;; settings fields to drag. The options differ only in their verdict.
(spawn-ground 0 -1 0)
(spawn-box 0 0.5 1 0.25)
(hinge (body-index-at 0 0.5) -1 -0.4 0.5)
(panel-show "properties")
(panel-show "settings")
