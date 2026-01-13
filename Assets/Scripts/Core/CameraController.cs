using UnityEngine;

namespace Core
{
    public class CameraController : MonoBehaviour
    {
        private Vector3 _dragOrigin;
        private bool _isDragging;

        [SerializeField]
        private float _zoomSpeed = 2f;

        [SerializeField]
        private float _minZoom = 1f;

        [SerializeField]
        private float _maxZoom = 50f;

        private void Update()
        {
            HandlePan();
            HandleZoom();
        }

        private void HandlePan()
        {
            // Pan with Middle Mouse Button (2)
            if (Input.GetMouseButtonDown(2))
            {
                _dragOrigin = Camera.main.ScreenToWorldPoint(Input.mousePosition);
                _isDragging = true;
            }

            if (Input.GetMouseButton(2) && _isDragging)
            {
                Vector3 difference = _dragOrigin - Camera.main.ScreenToWorldPoint(Input.mousePosition);
                Camera.main.transform.position += difference;
            }

            if (Input.GetMouseButtonUp(2))
            {
                _isDragging = false;
            }
        }

        private void HandleZoom()
        {
            float scroll = Input.GetAxis("Mouse ScrollWheel");
            if (scroll < -0.01f || scroll > 0.01f)
            {
                Camera.main.orthographicSize -= scroll * _zoomSpeed;
                Camera.main.orthographicSize = Mathf.Clamp(Camera.main.orthographicSize, _minZoom, _maxZoom);
            }
        }
    }
}
