using Core;
using Core.Commands;
using Physics;
using UnityEngine;

namespace Tools
{
    public class CircleTool : CreationTool
    {
        public override string ToolName => "Circle";

        protected override void OnDragStart(Vector2 position)
        {
            previewObject = PhysicsObjectFactory.CreateCircle(position, 0f);
            var rb = previewObject.GetComponent<Rigidbody2D>();
            if (rb)
            {
                rb.simulated = false;
            }
        }

        protected override void OnDrag(Vector2 position)
        {
            if (previewObject == null)
            {
                return;
            }

            float radius = Vector2.Distance(startPosition, position);
            radius = Mathf.Max(radius, 0.1f);

            var sr = previewObject.GetComponent<SpriteRenderer>();
            if (sr)
            {
                sr.size = new Vector2(radius * 2, radius * 2);
            }

            var col = previewObject.GetComponent<CircleCollider2D>();
            if (col)
            {
                col.radius = radius;
            }
        }

        protected override void OnDragEnd(Vector2 position)
        {
            if (previewObject == null)
            {
                return;
            }

            var rb = previewObject.GetComponent<Rigidbody2D>();
            if (rb)
            {
                rb.simulated = true;
            }

            var sr = previewObject.GetComponent<SpriteRenderer>();
            if (sr)
            {
                sr.color = Color.HSVToRGB(UnityEngine.Random.value, 0.6f, 0.9f);
            }

            // Register Undo
            if (CommandManager.Instance != null)
            {
                CommandManager.Instance.ExecuteCommand(new CreateObjectCommand(previewObject));
            }

            previewObject = null;
        }
    }
}
