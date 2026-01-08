using UnityEngine;
using Physics;

namespace Tools
{
    public class BoxTool : CreationTool
    {
        public override string ToolName => "Box";

        protected override void OnDragStart(Vector2 position)
        {
            PreviewObject = PhysicsObjectFactory.CreateBox(position, Vector2.zero);
            // Optional: Disable physics while drawing so it doesn't fall
            var rb = PreviewObject.GetComponent<Rigidbody2D>();
            if (rb) rb.simulated = false;
        }

        protected override void OnDrag(Vector2 position)
        {
            if (PreviewObject == null) return;

            Vector2 center = (StartPosition + position) / 2f;
            Vector2 size = new Vector2(Mathf.Abs(position.x - StartPosition.x), Mathf.Abs(position.y - StartPosition.y));

            // Avoid zero size
            size.x = Mathf.Max(size.x, 0.1f);
            size.y = Mathf.Max(size.y, 0.1f);

            PreviewObject.transform.position = center;

            var sr = PreviewObject.GetComponent<SpriteRenderer>();
            if (sr) sr.size = size;

            var col = PreviewObject.GetComponent<BoxCollider2D>();
            if (col) col.size = size;
        }

        protected override void OnDragEnd(Vector2 position)
        {
             if (PreviewObject == null) return;

             // Re-enable physics
             var rb = PreviewObject.GetComponent<Rigidbody2D>();
             if (rb) rb.simulated = true;

             // Random Color for Algodoo feel
             var sr = PreviewObject.GetComponent<SpriteRenderer>();
             if (sr) sr.color = Color.HSVToRGB(Random.value, 0.6f, 0.9f);

             PreviewObject = null;
        }
    }
}
