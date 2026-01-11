using Physics;
using UnityEngine;

namespace Tools
{
    public abstract class CreationTool : Tool
    {
        protected Vector2 startPosition;
        protected bool isDragging;
        protected GameObject? previewObject;

        protected static Vector2 GetMouseWorldPosition()
        {
            return Camera.main.ScreenToWorldPoint(Input.mousePosition);
        }

        public override void OnToolUpdate()
        {
            if (Input.GetMouseButtonDown(0))
            {
                startPosition = GetMouseWorldPosition();
                isDragging = true;
                OnDragStart(startPosition);
            }
            else if (Input.GetMouseButton(0) && isDragging)
            {
                OnDrag(GetMouseWorldPosition());
            }
            else if (Input.GetMouseButtonUp(0) && isDragging)
            {
                isDragging = false;
                OnDragEnd(GetMouseWorldPosition());
            }
        }

        protected abstract void OnDragStart(Vector2 position);

        protected abstract void OnDrag(Vector2 position);

        protected abstract void OnDragEnd(Vector2 position);
    }
}
