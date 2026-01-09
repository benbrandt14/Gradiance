using UnityEngine;

namespace Tools
{
    public class SpringTool : Tool
    {
        public override string ToolName => "Spring";

        private Rigidbody2D _firstBody;
        private Vector2 _firstAnchor;
        private bool _isDragging;

        private LineRenderer _previewLine;

        public override void OnToolSelected()
        {
             var go = new GameObject("SpringPreviewLine");
             _previewLine = go.AddComponent<LineRenderer>();
             _previewLine.startWidth = 0.05f;
             _previewLine.endWidth = 0.05f;
             _previewLine.material = new Material(Shader.Find("Sprites/Default"));
             _previewLine.startColor = Color.green;
             _previewLine.endColor = Color.green;
             _previewLine.gameObject.SetActive(false);
        }

        public override void OnToolDeselected()
        {
            if (_previewLine != null) Destroy(_previewLine.gameObject);
            _firstBody = null;
            _isDragging = false;
        }

        public override void OnToolUpdate()
        {
            var worldPos = (Vector2)Camera.main.ScreenToWorldPoint(Input.mousePosition);

            if (Input.GetMouseButtonDown(0))
            {
                var hit = Physics2D.Raycast(worldPos, Vector2.zero);
                _firstBody = hit.rigidbody; // Can be null (background)
                _firstAnchor = worldPos;
                _isDragging = true;

                _previewLine.gameObject.SetActive(true);
                _previewLine.SetPosition(0, _firstAnchor);
                _previewLine.SetPosition(1, _firstAnchor);
            }
            else if (Input.GetMouseButton(0) && _isDragging)
            {
                _previewLine.SetPosition(0, _firstAnchor);
                _previewLine.SetPosition(1, worldPos);
            }
            else if (Input.GetMouseButtonUp(0) && _isDragging)
            {
                var hit = Physics2D.Raycast(worldPos, Vector2.zero);
                var secondBody = hit.rigidbody;

                CreateSpring(_firstBody, _firstAnchor, secondBody, worldPos);

                _isDragging = false;
                _previewLine.gameObject.SetActive(false);
                _firstBody = null;
            }
        }

        private void CreateSpring(Rigidbody2D bodyA, Vector2 anchorA, Rigidbody2D bodyB, Vector2 anchorB)
        {
            // At least one body or two distinct points on background?
            // SpringJoint2D needs a host.
            // If both are null, we can't do much unless we make a dummy object.
            // Let's assume we need at least one body.

            if (bodyA == null && bodyB == null) return;

            var host = bodyA != null ? bodyA : bodyB;
            var connected = bodyA != null ? bodyB : null;

            // If we swapped (because bodyA was null), we must be careful with anchors.
            // But here:
            // Case 1: A valid, B valid. Host A, Connect B.
            // Case 2: A valid, B null. Host A, Connect null.
            // Case 3: A null, B valid. Host B, Connect null (wait, connect to A's point?).

            // It's cleaner to put the joint on the "First" body if it exists.

            SpringJoint2D joint;
            if (bodyA != null)
            {
                joint = bodyA.gameObject.AddComponent<SpringJoint2D>();
                joint.anchor = bodyA.transform.InverseTransformPoint(anchorA);
                joint.connectedBody = bodyB;
                if (bodyB != null)
                {
                    joint.connectedAnchor = bodyB.transform.InverseTransformPoint(anchorB);
                }
                else
                {
                    joint.connectedAnchor = anchorB;
                }
            }
            else // bodyA is null, bodyB must be valid
            {
                joint = bodyB.gameObject.AddComponent<SpringJoint2D>();
                joint.anchor = bodyB.transform.InverseTransformPoint(anchorB);
                joint.connectedBody = null; // Connect to world at A
                joint.connectedAnchor = anchorA;
            }

            joint.frequency = 2f;
            joint.dampingRatio = 0.1f;
            joint.autoConfigureDistance = true; // Use current distance as rest length

            // Visuals
            // Unity's SpringJoint2D doesn't draw itself. We need a script to draw the line.
            var visual = new GameObject("SpringVisual");
            visual.transform.SetParent(joint.transform);
            var springVis = visual.AddComponent<SpringVisualizer>();
            springVis.Initialize(joint);
        }
    }

    public class SpringVisualizer : MonoBehaviour
    {
        private SpringJoint2D _joint;
        private LineRenderer _lr;

        public void Initialize(SpringJoint2D joint)
        {
            _joint = joint;
            _lr = gameObject.AddComponent<LineRenderer>();
            _lr.startWidth = 0.05f;
            _lr.endWidth = 0.05f;
            _lr.material = new Material(Shader.Find("Sprites/Default"));
            _lr.startColor = Color.white;
            _lr.endColor = Color.white;
            // TODO: [Phase 3] Make it look like a zigzag spring instead of a straight line
        }

        private void Update()
        {
            if (_joint == null)
            {
                Destroy(gameObject);
                return;
            }

            // TODO: Handle case where connectedBody is destroyed?

            var p1 = _joint.transform.TransformPoint(_joint.anchor);
            var p2 = _joint.connectedBody != null ?
                _joint.connectedBody.transform.TransformPoint(_joint.connectedAnchor) :
                (Vector3)_joint.connectedAnchor;

            _lr.SetPosition(0, p1);
            _lr.SetPosition(1, p2);
        }
    }
}
