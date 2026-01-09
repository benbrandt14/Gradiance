using UnityEngine;

namespace Tools
{
    public class MoveTool : Tool
    {
        public override string ToolName => "Move";

        private Rigidbody2D _targetBody;
        private TargetJoint2D _targetJoint;

        public override void OnToolUpdate()
        {
            if (Input.GetMouseButtonDown(0))
            {
                var worldPos = Camera.main.ScreenToWorldPoint(Input.mousePosition);
                var hit = Physics2D.Raycast(worldPos, Vector2.zero);

                if (hit.collider != null && hit.rigidbody != null)
                {
                    _targetBody = hit.rigidbody;

                    // TODO: [Phase 1] Check if the body is static. If so, maybe don't drag it, or make it dynamic temporarily?
                    // Algodoo allows moving static objects in "edit" mode but not simulation.
                    // Gradiance is always running? Or do we distinguish Edit/Play mode?

                    // Add a TargetJoint2D to drag the object
                    _targetJoint = _targetBody.gameObject.AddComponent<TargetJoint2D>();
                    _targetJoint.anchor = _targetBody.transform.InverseTransformPoint(hit.point);
                    _targetJoint.target = worldPos;
                    _targetJoint.frequency = 5f; // Good response
                    _targetJoint.dampingRatio = 0.5f; // Little bit of springiness
                }
            }
            else if (Input.GetMouseButton(0) && _targetJoint != null)
            {
                var worldPos = Camera.main.ScreenToWorldPoint(Input.mousePosition);
                _targetJoint.target = worldPos;
            }
            else if (Input.GetMouseButtonUp(0) && _targetJoint != null)
            {
                Destroy(_targetJoint);
                _targetJoint = null;
                _targetBody = null;
            }

            // TODO: [Phase 1] Implement Right-Click Context Menu trigger here? Or in a separate Interaction Manager?
            // if (Input.GetMouseButtonDown(1)) { ... }
        }

        public override void OnToolDeselected()
        {
             // Cleanup if tool changed while dragging
             if (_targetJoint != null)
             {
                 Destroy(_targetJoint);
                 _targetJoint = null;
                 _targetBody = null;
             }
        }
    }
}
