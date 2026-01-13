using System;
using System.Collections.Generic;

namespace UnityEngine
{
    // Basic Types
    public struct Vector2
    {
        public float x;
        public float y;

        public Vector2(float x, float y)
        {
            this.x = x;
            this.y = y;
        }

        public static Vector2 zero => new Vector2(0, 0);

        public static Vector2 one => new Vector2(1, 1);

        public static Vector2 operator +(Vector2 a, Vector2 b) => new Vector2(a.x + b.x, a.y + b.y);

        public static Vector2 operator -(Vector2 a, Vector2 b) => new Vector2(a.x - b.x, a.y - b.y);

        public static Vector2 operator *(Vector2 a, float b) => new Vector2(a.x * b, a.y * b);

        public static Vector2 operator /(Vector2 a, float b) => new Vector2(a.x / b, a.y / b);

        public static float Distance(Vector2 a, Vector2 b) => 0f;
    }

    public struct Vector3
    {
        public float x;
        public float y;
        public float z;

        public Vector3(float x, float y, float z)
        {
            this.x = x;
            this.y = y;
            this.z = z;
        }

        public static Vector3 zero => new Vector3(0, 0, 0);

        public static Vector3 one => new Vector3(1, 1, 1);

        public static implicit operator Vector3(Vector2 v) => new Vector3(v.x, v.y, 0);

        public static implicit operator Vector2(Vector3 v) => new Vector2(v.x, v.y);

        public static Vector3 operator +(Vector3 a, Vector3 b) => new Vector3(a.x + b.x, a.y + b.y, a.z + b.z);

        public static Vector3 operator -(Vector3 a, Vector3 b) => new Vector3(a.x - b.x, a.y - b.y, a.z - b.z);

        public static float Distance(Vector3 a, Vector3 b)
        {
            float dx = a.x - b.x;
            float dy = a.y - b.y;
            float dz = a.z - b.z;
            return (float)Math.Sqrt((dx * dx) + (dy * dy) + (dz * dz));
        }
    }

    public struct Bounds
    {
        public Vector3 center;
        public Vector3 size;
    }

    public struct Quaternion
    {
        public float x;
        public float y;
        public float z;
        public float w;

        public Vector3 eulerAngles => Vector3.zero;

        public static Quaternion identity => new Quaternion { w = 1 };

        public static Quaternion Euler(float x, float y, float z) => identity;

        public static float Angle(Quaternion a, Quaternion b) => 0f;
    }

    public struct Color
    {
        public float r;
        public float g;
        public float b;
        public float a;

        public Color(float r, float g, float b, float a = 1)
        {
            this.r = r;
            this.g = g;
            this.b = b;
            this.a = a;
        }

        public static Color white => new Color { r = 1, g = 1, b = 1, a = 1 };

        public static Color black => new Color { r = 0, g = 0, b = 0, a = 1 };

        public static Color red => new Color { r = 1, g = 0, b = 0, a = 1 };

        public static Color green => new Color { r = 0, g = 1, b = 0, a = 1 };

        public static Color blue => new Color { r = 0, g = 0, b = 1, a = 1 };

        public static Color yellow => new Color { r = 1, g = 0.92f, b = 0.016f, a = 1 };

        public static Color gray => new Color { r = 0.5f, g = 0.5f, b = 0.5f, a = 1 };

        public static Color clear => new Color { r = 0, g = 0, b = 0, a = 0 };

        public static Color HSVToRGB(float h, float s, float v) => white;
    }

    public struct Rect
    {
        public float x;
        public float y;
        public float width;
        public float height;

        public Rect(float x, float y, float w, float h)
        {
            this.x = x;
            this.y = y;
            width = w;
            height = h;
        }
    }

    public struct RectOffset
    {
        public int left;
        public int right;
        public int top;
        public int bottom;

        public RectOffset(int l, int r, int t, int b)
        {
            left = l;
            right = r;
            top = t;
            bottom = b;
        }
    }

    public enum KeyCode
    {
        None,
        Space,
        Delete,
        Backspace,
        Return,
        Escape,
        LeftShift,
        RightShift,
        LeftControl,
        RightControl,
        Z,
        Y,
    }

    public class Random
    {
        public static float value => 0.5f;
    }

    public class Time
    {
        public static float timeScale { get; set; } = 1f;

        public static float deltaTime { get; set; } = 0.016f;
    }

    public class Mathf
    {
        public static float Abs(float f) => Math.Abs(f);

        public static float Sin(float f) => (float)Math.Sin(f);

        public static float Cos(float f) => (float)Math.Cos(f);

        public static float Atan2(float y, float x) => (float)Math.Atan2(y, x);

        public static float Max(float a, float b) => Math.Max(a, b);

        public static float Clamp(float value, float min, float max) => Math.Max(min, Math.Min(max, value));

        public static float Rad2Deg => 57.29578f;
    }

    public enum FindObjectsSortMode
    {
        None,
        InstanceID,
    }

    public class SerializeField : Attribute { }

    // Object & Component System
    public class Object
    {
        public string name;

        public static void Destroy(Object obj)
        {
            // Method intentionally left empty.
        }

        public static void DestroyImmediate(Object obj)
        {
            // Method intentionally left empty.
        }

        public static T Instantiate<T>(T original)
            where T : Object
            => original;

        public static T Instantiate<T>(T original, Transform parent)
            where T : Object
            => original;

        public static T FindObjectOfType<T>()
            where T : Object
            => null;

        public static T FindFirstObjectByType<T>()
            where T : Object
            => null;

        public static T[] FindObjectsOfType<T>()
            where T : Object
            => new T[0];

        public static T[] FindObjectsByType<T>(FindObjectsSortMode mode)
            where T : Object
            => new T[0];

        public static void DontDestroyOnLoad(Object target)
        {
            // Method intentionally left empty.
        }

        public static implicit operator bool(Object obj) => obj != null;
    }

    public class GameObject : Object
    {
        public Transform transform { get; } = new Transform();

        public int layer { get; set; }

        public string tag { get; set; }

        public GameObject()
        {
        }

        public GameObject(string name)
        {
            this.name = name;
        }

        public void SetActive(bool active)
        {
            // Method intentionally left empty.
        }

        private readonly List<Component> _components = new List<Component>();

        public T AddComponent<T>()
            where T : Component, new()
        {
            var comp = new T();
            comp.gameObject = this;
            _components.Add(comp);

            // Simulate Unity Lifecycle: Awake
            if (comp is MonoBehaviour)
            {
                var method = typeof(T).GetMethod("Awake", System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Public);
                if (method != null)
                {
                    method.Invoke(comp, null);
                }
            }

            return comp;
        }

        public T GetComponent<T>()
            where T : Component
        {
            foreach (var c in _components)
            {
                if (c is T t)
                {
                    return t;
                }
            }

            return null;
        }
    }

    public class Component : Object
    {
        public GameObject gameObject { get; set; }

        public Transform transform => gameObject?.transform;

        public T GetComponent<T>()
            where T : Component
            => gameObject?.GetComponent<T>();
    }

    public class MonoBehaviour : Component
    {
        public bool useGUILayout { get; set; }
    }

    public class Transform : Component
    {
        public Vector3 position { get; set; }

        public Quaternion rotation { get; set; }

        public Vector3 localScale { get; set; }

        public Transform parent { get; set; }

        public void SetParent(Transform p)
        {
            parent = p;
        }

        public void SetParent(Transform p, bool worldPositionStays)
        {
            parent = p;
        }

        public Vector3 InverseTransformPoint(Vector3 position) => position;

        public Vector3 TransformPoint(Vector3 position) => position;
    }

    public class PhysicsMaterial2D : Object
    {
        public PhysicsMaterial2D()
        {
        }

        public PhysicsMaterial2D(string name)
        {
            this.name = name;
        }

        public float friction { get; set; }

        public float bounciness { get; set; }
    }

    // Physics 2D
    public class Rigidbody2D : Component
    {
        public Vector2 position { get; set; }

        public float rotation { get; set; }

        public bool simulated { get; set; }

        public float mass { get; set; }

        public Vector2 linearVelocity { get; set; }

        public float angularVelocity { get; set; }

        public RigidbodyType2D bodyType { get; set; }

        public PhysicsMaterial2D sharedMaterial { get; set; }

        public void MoveRotation(float angle)
        {
            rotation = angle;
        }

        public static implicit operator bool(Rigidbody2D rb) => rb != null;
    }

    public enum RigidbodyType2D
    {
        Dynamic,
        Kinematic,
        Static,
    }

    public class Collider2D : Component
    {
        public PhysicsMaterial2D sharedMaterial { get; set; }

        public Bounds bounds { get; set; }
    }

    public class BoxCollider2D : Collider2D
    {
        public Vector2 size { get; set; }
    }

    public class CircleCollider2D : Collider2D
    {
        public float radius { get; set; }
    }

    public class Joint2D : Component
    {
        public Rigidbody2D connectedBody { get; set; }
    }

    public class TargetJoint2D : Joint2D
    {
        public Vector2 anchor { get; set; }

        public Vector2 target { get; set; }

        public float frequency { get; set; }

        public float dampingRatio { get; set; }
    }

    public class HingeJoint2D : Joint2D
    {
        public Vector2 anchor { get; set; }
    }

    public class SpringJoint2D : Joint2D
    {
        public Vector2 anchor { get; set; }

        public Vector2 connectedAnchor { get; set; }

        public float frequency { get; set; }

        public float dampingRatio { get; set; }

        public bool autoConfigureDistance { get; set; }
    }

    public class DistanceJoint2D : Joint2D
    {
    }

    public class Physics2D
    {
        public static Vector2 gravity { get; set; } = new Vector2(0, -9.81f);

        public static RaycastHit2D Raycast(Vector2 origin, Vector2 direction) => default(RaycastHit2D);

        public static RaycastHit2D[] RaycastAll(Vector2 origin, Vector2 direction) => new RaycastHit2D[0];
    }

    public struct RaycastHit2D
    {
        public Collider2D collider;
        public Transform transform;
        public Rigidbody2D rigidbody;
        public Vector2 point;
    }

    // Graphics & UI
    public class Texture2D : Object
    {
        public Texture2D(int width, int height)
        {
        }

        public void SetPixels(Color[] colors)
        {
            // Method intentionally left empty.
        }

        public void Apply()
        {
            // Method intentionally left empty.
        }
    }

    public class Sprite : Object
    {
        public static Sprite Create(Texture2D texture, Rect rect, Vector2 pivot) => new Sprite();

        public static Sprite Create(Texture2D texture, Rect rect, Vector2 pivot, float pixelsPerUnit) => new Sprite();
    }

    public enum SpriteDrawMode
    {
        Simple,
        Sliced,
        Tiled,
    }

    public class SpriteRenderer : Component
    {
        public Color color { get; set; }

        public Sprite sprite { get; set; }

        public SpriteDrawMode drawMode { get; set; }

        public int sortingOrder { get; set; }

        public Vector2 size { get; set; }

        public static implicit operator bool(SpriteRenderer sr) => sr != null;
    }

    public class Material : Object
    {
        public Material(Shader shader)
        {
        }
    }

    public class Shader : Object
    {
        public static Shader Find(string name) => new Shader();
    }

    public class LineRenderer : Component
    {
        public int positionCount { get; set; }

        public void SetPosition(int index, Vector3 position)
        {
            // Method intentionally left empty.
        }

        public float startWidth { get; set; }

        public float endWidth { get; set; }

        public Material material { get; set; }

        public Color startColor { get; set; }

        public Color endColor { get; set; }
    }

    public enum CameraClearFlags
    {
        SolidColor,
    }

    public class Camera : Component
    {
        public static Camera main => new Camera();

        public Vector3 ScreenToWorldPoint(Vector3 position) => position;

        public bool orthographic { get; set; }

        public float orthographicSize { get; set; }

        public CameraClearFlags clearFlags { get; set; }

        public Color backgroundColor { get; set; }
    }

    public enum RenderMode
    {
        ScreenSpaceOverlay,
    }

    public class Canvas : Component
    {
        public RenderMode renderMode { get; set; }

        public int sortingOrder { get; set; }
    }

    public class RectTransform : Transform
    {
        public Vector2 anchorMin { get; set; }

        public Vector2 anchorMax { get; set; }

        public Vector2 offsetMin { get; set; }

        public Vector2 offsetMax { get; set; }

        public Vector2 sizeDelta { get; set; }

        public Vector2 pivot { get; set; }

        public Vector2 anchoredPosition { get; set; }
    }

    public class Debug
    {
        public static void Log(object message) => Console.WriteLine($"[Log] {message}");

        public static void LogWarning(object message) => Console.WriteLine($"[Warning] {message}");

        public static void LogError(object message) => Console.WriteLine($"[Error] {message}");
    }

    public class Input
    {
        public static Vector3 mousePosition { get; set; }

        public static bool GetMouseButtonDown(int button) => false;

        public static bool GetMouseButtonUp(int button) => false;

        public static bool GetMouseButton(int button) => false;

        public static bool GetKeyDown(KeyCode key) => false;

        public static bool GetKey(KeyCode key) => false;

        public static float GetAxis(string axisName) => 0f;
    }

    public class Resources
    {
        public static T Load<T>(string path)
            where T : Object
            => null;

        public static T GetBuiltinResource<T>(string path)
            where T : Object
            => null;
    }

    public class Font : Object
    {
        public static Font CreateDynamicFontFromOSFont(string fontname, int size) => new Font();
    }

    // Initialization
    public class RuntimeInitializeOnLoadMethodAttribute : Attribute
    {
        public RuntimeInitializeOnLoadMethodAttribute()
        {
        }

        public RuntimeInitializeOnLoadMethodAttribute(RuntimeInitializeLoadType type)
        {
        }
    }

    public enum RuntimeInitializeLoadType
    {
        AfterSceneLoad,
    }
}

namespace UnityEngine.Events
{
    public delegate void UnityAction();

    public class UnityEvent : UnityEngine.Object
    {
        public void AddListener(UnityAction call)
        {
            // Method intentionally left empty.
        }
    }

    public class UnityEvent<T> : UnityEngine.Object
    {
        public void AddListener(Action<T> call)
        {
            // Method intentionally left empty.
        }
    }
}

namespace UnityEngine.EventSystems
{
    public class EventSystem : MonoBehaviour
    {
        public static EventSystem current { get; set; }

        public bool IsPointerOverGameObject() => false;
    }

    public class StandaloneInputModule : MonoBehaviour
    {
    }
}

namespace UnityEngine.UI
{
    public enum TextAnchor
    {
        UpperLeft,
        MiddleCenter,
        MiddleLeft,
    }

    public class Text : MonoBehaviour
    {
        public string text { get; set; }

        public Font font { get; set; }

        public Color color { get; set; }

        public TextAnchor alignment { get; set; }

        public bool resizeTextForBestFit { get; set; }
    }

    public class Image : MonoBehaviour
    {
        public Color color { get; set; }

        public UnityEngine.Sprite sprite { get; set; }

        public UnityEngine.Object targetGraphic { get; set; }
    }

    // Image doesn't have targetGraphic, Selectable does. Button/Slider inherit Selectable.
    public class Selectable : MonoBehaviour
    {
        public Image targetGraphic { get; set; }
    }

    public class Button : Selectable
    {
        public struct ButtonClickedEvent
        {
            public void AddListener(UnityEngine.Events.UnityAction call)
            {
                // Method intentionally left empty.
            }
        }

        public ButtonClickedEvent onClick;
    }

    public class Slider : Selectable
    {
        public float value { get; set; }

        public float minValue { get; set; }

        public float maxValue { get; set; }

        public UnityEngine.Events.UnityEvent<float> onValueChanged;

        public RectTransform handleRect { get; set; }
    }

    public class LayoutElement : MonoBehaviour
    {
        public float minWidth { get; set; }

        public float minHeight { get; set; }

        public float flexibleWidth { get; set; }

        public float preferredWidth { get; set; }
    }

    public class CanvasScaler : MonoBehaviour
    {
        public enum ScaleMode
        {
            ScaleWithScreenSize,
        }

        public ScaleMode uiScaleMode { get; set; }

        public Vector2 referenceResolution { get; set; }
    }

    public class GraphicRaycaster : MonoBehaviour
    {
    }

    public class VerticalLayoutGroup : MonoBehaviour
    {
        public UnityEngine.RectOffset padding { get; set; }

        public float spacing { get; set; }

        public bool childControlHeight { get; set; }

        public bool childForceExpandHeight { get; set; }
    }

    public class HorizontalLayoutGroup : MonoBehaviour
    {
        public UnityEngine.RectOffset padding { get; set; }

        public float spacing { get; set; }

        public bool childControlWidth { get; set; }

        public bool childForceExpandWidth { get; set; }

        public TextAnchor childAlignment { get; set; }
    }
}
