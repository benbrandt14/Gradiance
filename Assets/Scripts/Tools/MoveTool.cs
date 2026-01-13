using Core;
using Core.Commands;
using UI;
using UnityEngine;

namespace Tools
{
    public class MoveTool : Tool
    {
        private Rigidbody2D? _draggingBody;
        private TargetJoint2D? _targetJoint;
        private bool _isRotating;
        private float _rotationOffset;

        // Undo State Tracking
        private Vector3 _startPosition;
        private Quaternion _startRotation;

        private Rigidbody2D? _hoveredBody;
        private Rigidbody2D? _selectedBody;

        private GameObject? _selectionHalo;

        public override string ToolName => "Move";

        public override void OnToolSelected()
        {
            // Create a selection visualization
            _selectionHalo = new GameObject("SelectionHalo");
            var sr = _selectionHalo.AddComponent<SpriteRenderer>();
            sr.sprite = Core.GraphicsHelper.CreateCircleSprite(64, new Color(0, 1, 0, 0.5f)); // Semi-transparent green
            sr.drawMode = SpriteDrawMode.Sliced;
            sr.size = new Vector2(1.2f, 1.2f);
            sr.sortingOrder = 100;
            _selectionHalo.SetActive(false);
        }

        public override void OnToolDeselected()
        {
            // Cleanup if tool changed while dragging
            if (_targetJoint != null)
            {
                Destroy(_targetJoint);
                _targetJoint = null;
                _draggingBody = null;
            }

            if (_selectionHalo != null)
            {
                Destroy(_selectionHalo);
                _selectionHalo = null;
            }

            _selectedBody = null;
            _hoveredBody = null;
        }

        public override void OnToolUpdate()
        {
            var worldPos = (Vector2)Camera.main.ScreenToWorldPoint(Input.mousePosition);

            // 1. Hover Logic (Raycast)
            // We only update hover if we are not currently dragging
            if (_targetJoint == null)
            {
                var hit = Physics2D.Raycast(worldPos, Vector2.zero);
                _hoveredBody = (hit.collider != null) ? hit.rigidbody : null;
            }

            // 2. Selection Visualization Update
            UpdateSelectionVisuals();

            // 3. Input Handling
            if (Input.GetMouseButtonDown(0))
            {
                HandleLeftClickDown(worldPos);
            }
            else if (Input.GetMouseButton(0))
            {
                if (_isRotating && _draggingBody != null)
                {
                    PerformRotation(worldPos);
                }
                else if (_targetJoint != null)
                {
                    _targetJoint.target = worldPos;
                }
            }

            if (Input.GetMouseButtonUp(0))
            {
                StopDragging();
            }

            // Right Click for Context Menu
            if (Input.GetMouseButtonDown(1))
            {
                HandleRightClick(worldPos);
            }
        }

        private void HandleLeftClickDown(Vector2 worldPos)
        {
            if (_hoveredBody != null)
            {
                _draggingBody = _hoveredBody;
                _selectedBody = _hoveredBody;

                // Capture start state for Undo
                _startPosition = _draggingBody.transform.position;
                _startRotation = _draggingBody.transform.rotation;

                if (Input.GetKey(KeyCode.LeftShift))
                {
                    // Start Rotation
                    _isRotating = true;
                    Vector2 dir = worldPos - _draggingBody.position;
                    float angle = Mathf.Atan2(dir.y, dir.x) * Mathf.Rad2Deg;
                    _rotationOffset = angle - _draggingBody.rotation;
                }
                else
                {
                    // Start Dragging
                    _targetJoint = _draggingBody.gameObject.AddComponent<TargetJoint2D>();
                    _targetJoint.anchor = _draggingBody.transform.InverseTransformPoint(worldPos);
                    _targetJoint.target = worldPos;
                    _targetJoint.frequency = 10f;
                    _targetJoint.dampingRatio = 1f;
                }
            }
            else
            {
                // Clicked on nothing -> Deselect
                _selectedBody = null;
            }
        }

        private void PerformRotation(Vector2 worldPos)
        {
            if (_draggingBody == null)
            {
                return;
            }

            Vector2 dir = worldPos - _draggingBody.position;
            float angle = Mathf.Atan2(dir.y, dir.x) * Mathf.Rad2Deg;
            _draggingBody.MoveRotation(angle - _rotationOffset);
        }

        private void StopDragging()
        {
            if (_targetJoint != null)
            {
                Destroy(_targetJoint);
                _targetJoint = null;
            }

            // Register Undo if we actually moved/rotated
            if (_draggingBody != null &&
               (Vector3.Distance(_startPosition, _draggingBody.transform.position) > 0.001f ||
                Quaternion.Angle(_startRotation, _draggingBody.transform.rotation) > 0.001f) &&
                CommandManager.Instance != null)
            {
                CommandManager.Instance.ExecuteCommand(new MoveObjectCommand(
                    _draggingBody.transform,
                    _startPosition,
                    _startRotation,
                    _draggingBody.transform.position,
                    _draggingBody.transform.rotation));
            }

            _isRotating = false;
            _draggingBody = null;
        }

        private void HandleRightClick(Vector2 worldPos)
        {
            // If we right-click an object, select it and show menu
            // If we right-click nothing, but have a selection, maybe show menu for that?
            // Standard behavior: Context menu is for what is under cursor.
            var hit = Physics2D.Raycast(worldPos, Vector2.zero);
            var target = (hit.collider != null) ? hit.rigidbody : null;

            if (target != null)
            {
                _selectedBody = target;
                var ctx = UnityEngine.Object.FindFirstObjectByType<ContextMenuController>();
                if (ctx != null)
                {
                    ctx.Show(target, Input.mousePosition);
                }
            }
        }

        private void UpdateSelectionVisuals()
        {
            if (_selectionHalo == null)
            {
                return;
            }

            // Priority: Hovered > Selected
            var target = _hoveredBody ?? _selectedBody;

            if (target != null)
            {
                _selectionHalo.SetActive(true);
                _selectionHalo.transform.position = target.transform.position;
                _selectionHalo.transform.rotation = target.transform.rotation;

                // Try to match size (approximate)
                // A simple way is to check the collider bounds if possible, or just scale.
                // Assuming typical objects are around 1 unit.
                // Note: Getting collider bounds requires checking attached colliders.
                var col = target.GetComponent<Collider2D>();
                if (col != null)
                {
                    var bounds = col.bounds;
                    float maxDim = Mathf.Max(bounds.size.x, bounds.size.y);
                    _selectionHalo.transform.localScale = new Vector3(maxDim * 1.2f, maxDim * 1.2f, 1f);
                }
                else
                {
                    _selectionHalo.transform.localScale = Vector3.one;
                }

                // Change color slightly if it's selection vs hover?
                var sr = _selectionHalo.GetComponent<SpriteRenderer>();
                if (Input.GetKey(KeyCode.LeftShift) && (_hoveredBody != null || _selectedBody != null))
                {
                    sr.color = new Color(1, 1, 0, 0.5f); // Yellow for rotation
                }
                else if (_hoveredBody != null)
                {
                    sr.color = new Color(1, 1, 1, 0.3f); // White hover
                }
                else
                {
                    sr.color = new Color(0, 1, 0, 0.4f); // Green select
                }
            }
            else
            {
                _selectionHalo.SetActive(false);
            }
        }
    }
}
