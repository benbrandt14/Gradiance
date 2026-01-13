using Core;
using Core.Commands;
using Physics;
using UnityEngine;

namespace Tools
{
    public class BoxTool : CreationTool
    {
        public override string ToolName => "Box";

        protected override void OnDragStart(Vector2 position)
        {
            previewObject = PhysicsObjectFactory.CreateBox(position, Vector2.zero);

            // Optional: Disable physics while drawing so it doesn't fall
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

            Vector2 center = (startPosition + position) / 2f;
            Vector2 size = new Vector2(Mathf.Abs(position.x - startPosition.x), Mathf.Abs(position.y - startPosition.y));

            // Avoid zero size
            size.x = Mathf.Max(size.x, 0.1f);
            size.y = Mathf.Max(size.y, 0.1f);

            previewObject.transform.position = center;

            var sr = previewObject.GetComponent<SpriteRenderer>();
            if (sr)
            {
                sr.size = size;
            }

            var col = previewObject.GetComponent<BoxCollider2D>();
            if (col)
            {
                col.size = size;
            }
        }

        protected override void OnDragEnd(Vector2 position)
        {
            if (previewObject == null)
            {
                return;
            }

            // Re-enable physics
            var rb = previewObject.GetComponent<Rigidbody2D>();
            if (rb)
            {
                rb.simulated = true;
            }

            // Random Color for Algodoo feel
            var sr = previewObject.GetComponent<SpriteRenderer>();
            if (sr)
            {
                sr.color = Color.HSVToRGB(UnityEngine.Random.value, 0.6f, 0.9f);
            }

            // Register Undo
            if (CommandManager.Instance != null)
            {
                // The object is already created/active, so we just push the command
                // The Command's Execute just sets Active(true), which is redundant now but correct for Redo.
                CommandManager.Instance.ExecuteCommand(new CreateObjectCommand(previewObject));
            }

            previewObject = null;
        }
    }
}
