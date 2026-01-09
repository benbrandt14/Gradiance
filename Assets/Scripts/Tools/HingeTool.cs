using UnityEngine;

namespace Tools
{
    public class HingeTool : Tool
    {
        public override string ToolName => "Hinge";

        public override void OnToolUpdate()
        {
            if (Input.GetMouseButtonDown(0))
            {
                var worldPos = (Vector2)Camera.main.ScreenToWorldPoint(Input.mousePosition);

                // Raycast to find what we clicked on
                var hits = Physics2D.RaycastAll(worldPos, Vector2.zero);

                Rigidbody2D bodyA = null;
                Rigidbody2D bodyB = null;

                // Simple logic: If we hit something, attach to it.
                // If we hit two things (overlapping), attach them together.
                // If we hit one thing, attach it to background (static).

                foreach (var hit in hits)
                {
                     if (hit.rigidbody != null)
                     {
                         if (bodyA == null) bodyA = hit.rigidbody;
                         else if (bodyB == null && hit.rigidbody != bodyA) bodyB = hit.rigidbody;
                     }
                }

                CreateHinge(worldPos, bodyA, bodyB);
            }
        }

        private void CreateHinge(Vector2 anchor, Rigidbody2D bodyA, Rigidbody2D bodyB)
        {
            // If no bodies hit, we might just be clicking empty space. Do nothing or create a dummy anchor?
            // Algodoo usually requires at least one body.
            if (bodyA == null && bodyB == null) return;

            // Determine host for the joint
            var jointHost = bodyA != null ? bodyA.gameObject : bodyB.gameObject;
            var connectedBody = bodyA != null ? bodyB : bodyA; // The other one, or null (background)

            // Special case: if we have two bodies, one might be the host.
            // But HingeJoint2D is attached to one GameObject and connects to another (or null/world).
            // Let's attach to BodyA.

            var joint = jointHost.AddComponent<HingeJoint2D>();
            joint.anchor = jointHost.transform.InverseTransformPoint(anchor);
            joint.connectedBody = connectedBody;

            // TODO: [Phase 1] Check if we need to set 'enableCollision' (collideConnected) to false by default?
            // Usually hinge joints disable collision between the connected bodies.

            // Visuals: Create a small circle to represent the hinge
            var hingeVis = new GameObject("HingeVisual");
            hingeVis.transform.position = anchor;
            hingeVis.transform.SetParent(jointHost.transform); // Move with the host

            var sr = hingeVis.AddComponent<SpriteRenderer>();
            // Use a small circle
            sr.sprite = Resources.Load<Sprite>("Knob"); // Default Unity knob or create one
            if (sr.sprite == null)
            {
                // Fallback: create a tiny circle procedurally or use what we have
                // We don't have easy access to PhysicsObjectFactory private sprites, maybe make them public?
                // For now, let's just make a tiny circle primitive
                sr.sprite = CreateCircleSprite();
            }
            sr.drawMode = SpriteDrawMode.Sliced;
            sr.sortingOrder = 10; // On top
            sr.color = Color.yellow;
            sr.size = new Vector2(0.2f, 0.2f);
        }

        private Sprite CreateCircleSprite()
        {
            // Quick 32x32 circle
             int res = 32;
             var tex = new Texture2D(res, res);
             var cols = new Color[res*res];
             float c = res/2f;
             float rSq = (c-1)*(c-1);
             for(int y=0; y<res; y++)
             {
                 for(int x=0; x<res; x++)
                 {
                     float d = (x-c)*(x-c) + (y-c)*(y-c);
                     cols[y*res+x] = (d < rSq) ? Color.white : Color.clear;
                 }
             }
             tex.SetPixels(cols);
             tex.Apply();
             return Sprite.Create(tex, new Rect(0,0,res,res), new Vector2(0.5f,0.5f), res);
        }
    }
}
