using UnityEngine;

namespace Tools
{
    public class SpringVisualizer : MonoBehaviour
    {
        private SpringJoint2D? _joint;
        private LineRenderer? _lr;

        public void Initialize(SpringJoint2D joint)
        {
            _joint = joint;
            _lr = gameObject.AddComponent<LineRenderer>();
            _lr.startWidth = 0.05f;
            _lr.endWidth = 0.05f;
            _lr.material = new Material(Shader.Find("Sprites/Default"));
            _lr.startColor = Color.white;
            _lr.endColor = Color.white;
        }

        private void Update()
        {
            if (_joint == null)
            {
                Destroy(gameObject);
                return;
            }

            if (_lr == null)
            {
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
