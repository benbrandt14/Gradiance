using UnityEngine;

namespace Physics
{
    public static class PhysicsObjectFactory
    {
        private static Sprite _squareSprite;
        private static Sprite _circleSprite;

        public static GameObject CreateBox(Vector2 position, Vector2 size)
        {
            var go = new GameObject("Box");
            go.transform.position = position;

            // Sprite Renderer
            var sr = go.AddComponent<SpriteRenderer>();
            sr.sprite = GetSquareSprite();
            sr.drawMode = SpriteDrawMode.Sliced;
            sr.size = size;
            sr.color = Color.white; // Can be randomized later

            // Collider
            var col = go.AddComponent<BoxCollider2D>();
            col.size = size;

            // Rigidbody
            var rb = go.AddComponent<Rigidbody2D>();

            return go;
        }

        public static GameObject CreateCircle(Vector2 position, float radius)
        {
            var go = new GameObject("Circle");
            go.transform.position = position;

            // Sprite Renderer
            var sr = go.AddComponent<SpriteRenderer>();
            sr.sprite = GetCircleSprite();
            sr.drawMode = SpriteDrawMode.Sliced;
            sr.size = new Vector2(radius * 2, radius * 2); // Sprite size is diameter
            sr.color = Color.white;

            // Collider
            var col = go.AddComponent<CircleCollider2D>();
            col.radius = radius;

            // Rigidbody
            var rb = go.AddComponent<Rigidbody2D>();

            return go;
        }

        private static Sprite GetSquareSprite()
        {
            if (_squareSprite == null)
            {
                var texture = new Texture2D(2, 2);
                texture.SetPixels(new[] {Color.white, Color.white, Color.white, Color.white});
                texture.Apply();
                // 1 pixel per unit for easy scaling? No, defaults usually 100.
                // Let's make it 100 so 1x1 scale is 1 unit.
                _squareSprite = Sprite.Create(texture, new Rect(0, 0, 2, 2), new Vector2(0.5f, 0.5f), 1f);
            }
            return _squareSprite;
        }

        private static Sprite GetCircleSprite()
        {
             if (_circleSprite == null)
            {
                // Procedurally generate a simple circle texture
                int res = 64;
                var texture = new Texture2D(res, res);
                Color[] colors = new Color[res * res];
                float center = res / 2f;
                float radiusSqr = (res / 2f - 1) * (res / 2f - 1); // -1 for padding

                for (int y = 0; y < res; y++)
                {
                    for (int x = 0; x < res; x++)
                    {
                        float distSqr = (x - center) * (x - center) + (y - center) * (y - center);
                        if (distSqr < radiusSqr)
                        {
                            colors[y * res + x] = Color.white;
                        }
                        else
                        {
                            colors[y * res + x] = Color.clear;
                        }
                    }
                }
                texture.SetPixels(colors);
                texture.Apply();
                _circleSprite = Sprite.Create(texture, new Rect(0, 0, res, res), new Vector2(0.5f, 0.5f), res);
            }
            return _circleSprite;
        }
    }
}
