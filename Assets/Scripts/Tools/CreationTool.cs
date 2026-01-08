using UnityEngine;
using Physics;

namespace Tools
{
    public abstract class CreationTool : Tool
    {
        protected Vector2 StartPosition;
        protected bool IsDragging;
        protected GameObject PreviewObject;

        public override void OnToolUpdate()
        {
            if (Input.GetMouseButtonDown(0))
            {
                StartPosition = GetMouseWorldPosition();
                IsDragging = true;
                OnDragStart(StartPosition);
            }
            else if (Input.GetMouseButton(0) && IsDragging)
            {
                OnDrag(GetMouseWorldPosition());
            }
            else if (Input.GetMouseButtonUp(0) && IsDragging)
            {
                IsDragging = false;
                OnDragEnd(GetMouseWorldPosition());
            }
        }

        protected abstract void OnDragStart(Vector2 position);
        protected abstract void OnDrag(Vector2 position);
        protected abstract void OnDragEnd(Vector2 position);

        protected Vector2 GetMouseWorldPosition()
        {
            return Camera.main.ScreenToWorldPoint(Input.mousePosition);
        }
    }
}
