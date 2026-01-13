using UnityEngine;

namespace Core.Commands
{
    public class MoveObjectCommand : ICommand
    {
        private readonly Transform _transform;
        private readonly Vector3 _startPosition;
        private readonly Quaternion _startRotation;
        private readonly Vector3 _endPosition;
        private readonly Quaternion _endRotation;
        private readonly Rigidbody2D _rb;

        public MoveObjectCommand(Transform transform, Vector3 startPos, Quaternion startRot, Vector3 endPos, Quaternion endRot)
        {
            _transform = transform;
            _startPosition = startPos;
            _startRotation = startRot;
            _endPosition = endPos;
            _endRotation = endRot;
            _rb = transform.GetComponent<Rigidbody2D>();
        }

        public void Execute()
        {
            if (_transform != null)
            {
                _transform.position = _endPosition;
                _transform.rotation = _endRotation;
                SyncRigidbody();
            }
        }

        public void Undo()
        {
            if (_transform != null)
            {
                _transform.position = _startPosition;
                _transform.rotation = _startRotation;
                SyncRigidbody();
            }
        }

        private void SyncRigidbody()
        {
            if (_rb != null)
            {
                // If the object is simulated, we need to update the RB position/rotation
                // otherwise physics engine might override transform change next frame
                _rb.position = _transform.position;
                _rb.rotation = _transform.rotation.eulerAngles.z;
                _rb.linearVelocity = Vector2.zero;
                _rb.angularVelocity = 0f;
            }
        }
    }
}
