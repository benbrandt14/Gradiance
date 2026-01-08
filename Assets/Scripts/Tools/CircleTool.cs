using UnityEngine;
using Physics;

namespace Tools
{
    public class CircleTool : CreationTool
    {
        public override string ToolName => "Circle";

        protected override void OnDragStart(Vector2 position)
        {
            PreviewObject = PhysicsObjectFactory.CreateCircle(position, 0f);
            var rb = PreviewObject.GetComponent<Rigidbody2D>();
            if (rb) rb.simulated = false;
        }

        protected override void OnDrag(Vector2 position)
        {
            if (PreviewObject == null) return;

            float radius = Vector2.Distance(StartPosition, position);
            radius = Mathf.Max(radius, 0.1f);

            var sr = PreviewObject.GetComponent<SpriteRenderer>();
            if (sr) sr.size = new Vector2(radius * 2, radius * 2);

            var col = PreviewObject.GetComponent<CircleCollider2D>();
            if (col) col.radius = radius;
        }

        protected override void OnDragEnd(Vector2 position)
        {
            if (PreviewObject == null) return;

            var rb = PreviewObject.GetComponent<Rigidbody2D>();
            if (rb) rb.simulated = true;

            var sr = PreviewObject.GetComponent<SpriteRenderer>();
            if (sr) sr.color = Color.HSVToRGB(Random.value, 0.6f, 0.9f);

            PreviewObject = null;
        }
    }
}
