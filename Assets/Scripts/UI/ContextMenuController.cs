using UnityEngine;
using UnityEngine.EventSystems;
using UnityEngine.UI;

namespace UI
{
    public class ContextMenuController : MonoBehaviour
    {
        private GameObject? _panel;
        private Rigidbody2D? _target;

        // UI Elements
        private Slider? _frictionSlider;
        private Slider? _bouncinessSlider;
        private Slider? _massSlider; // Density

        private static bool IsPointerOverUI()
        {
            return EventSystem.current.IsPointerOverGameObject();
        }

        private static void CreateText(string content, Transform parent)
        {
            var go = new GameObject("Label");
            go.transform.SetParent(parent, false);
            var txt = go.AddComponent<Text>();
            txt.text = content;
            txt.font = Resources.GetBuiltinResource<Font>("Arial.ttf");
            txt.color = Color.white;
            txt.alignment = TextAnchor.MiddleLeft;
            go.AddComponent<LayoutElement>().minHeight = 20;
        }

        private static Slider CreateSlider(float min, float max, Transform parent, System.Action<float> onChanged)
        {
            var go = new GameObject("Slider");
            go.transform.SetParent(parent, false);
            var layout = go.AddComponent<LayoutElement>();
            layout.minHeight = 30;
            layout.preferredWidth = 200;

            // Slider needs a background and a handle/fill
            // This is tedious in code. We'll make a barebones one.
            var slider = go.AddComponent<Slider>();
            slider.minValue = min;
            slider.maxValue = max;

            // Background
            var bg = new GameObject("Background");
            bg.transform.SetParent(go.transform, false);
            var bgImg = bg.AddComponent<Image>();
            bgImg.color = Color.gray;
            var bgRect = bg.GetComponent<RectTransform>();
            bgRect.anchorMin = new Vector2(0, 0.25f);
            bgRect.anchorMax = new Vector2(1, 0.75f);
            bgRect.offsetMin = Vector2.zero;
            bgRect.offsetMax = Vector2.zero;
            slider.targetGraphic = bgImg; // Just something to target

            // Fill Area (skip for speed, just Handle)
            var handleArea = new GameObject("HandleArea");
            handleArea.transform.SetParent(go.transform, false);
            var haRect = handleArea.AddComponent<RectTransform>();
            haRect.anchorMin = Vector2.zero;
            haRect.anchorMax = Vector2.one;
            haRect.offsetMin = new Vector2(10, 0); // Padding
            haRect.offsetMax = new Vector2(-10, 0);

            var handle = new GameObject("Handle");
            handle.transform.SetParent(handleArea.transform, false);
            var hImg = handle.AddComponent<Image>();
            hImg.color = Color.white;
            var hRect = handle.GetComponent<RectTransform>();
            hRect.sizeDelta = new Vector2(20, 0);
            hRect.anchorMin = new Vector2(0, 0);
            hRect.anchorMax = new Vector2(0, 1);

            slider.handleRect = hRect;
            slider.onValueChanged.AddListener((v) => onChanged(v));

            return slider;
        }

        private static void CreateButton(string text, Transform parent, UnityEngine.Events.UnityAction action)
        {
            var go = new GameObject("Btn");
            go.transform.SetParent(parent, false);
            var img = go.AddComponent<Image>();
            img.color = new Color(0.4f, 0.4f, 0.4f);
            var btn = go.AddComponent<Button>();
            btn.onClick.AddListener(action);
            go.AddComponent<LayoutElement>().minHeight = 30;

            var tGo = new GameObject("Text");
            tGo.transform.SetParent(go.transform, false);
            var txt = tGo.AddComponent<Text>();
            txt.text = text;
            txt.font = Resources.GetBuiltinResource<Font>("Arial.ttf");
            txt.color = Color.white;
            txt.alignment = TextAnchor.MiddleCenter;
            var tr = tGo.GetComponent<RectTransform>();
            tr.anchorMin = Vector2.zero;
            tr.anchorMax = Vector2.one;
        }

        private void Awake()
        {
            BuildUI();
        }

        private void Update()
        {
            // Close if clicked outside (Left click) and not on UI
            // Note: We leave Right Click detection to the Tools (like MoveTool) now.
            if (Input.GetMouseButtonDown(0) && !IsPointerOverUI())
            {
                Hide();
            }
        }

        private void BuildUI()
        {
            // Background Panel
            _panel = gameObject;
            var img = _panel.AddComponent<Image>();
            img.color = new Color(0.2f, 0.2f, 0.2f, 0.95f);

            var rect = _panel.GetComponent<RectTransform>();
            rect.sizeDelta = new Vector2(250, 300);
            rect.pivot = new Vector2(0, 1); // Top Left pivot for easy positioning

            var layout = _panel.AddComponent<VerticalLayoutGroup>();
            layout.padding = new RectOffset(10, 10, 10, 10);
            layout.spacing = 5;
            layout.childControlHeight = false;
            layout.childForceExpandHeight = false;

            // Header
            CreateText("Properties", transform);

            // Friction
            CreateText("Friction", transform);
            _frictionSlider = CreateSlider(0, 1, transform, (val) =>
            {
                if (_target != null && _target.sharedMaterial != null)
                {
                    _target.sharedMaterial.friction = val;
                }
            });

            // Bounciness
            CreateText("Bounciness", transform);
            _bouncinessSlider = CreateSlider(0, 1.2f, transform, (val) =>
            {
                if (_target != null && _target.sharedMaterial != null)
                {
                    _target.sharedMaterial.bounciness = val;
                }
            });

            // Density (Mass)
            CreateText("Density", transform);
            _massSlider = CreateSlider(0.1f, 10f, transform, (val) =>
            {
                if (_target != null)
                {
                    _target.mass = val; // Actually mass, but close enough
                }
            });

            // Color Randomizer Button
            CreateButton("Random Color", transform, () =>
            {
                if (_target != null)
                {
                    var sr = _target.GetComponent<SpriteRenderer>();
                    if (sr)
                    {
                        sr.color = Color.HSVToRGB(UnityEngine.Random.value, 0.7f, 0.9f);
                    }
                }
            });

            // Delete Button
            CreateButton("Delete", transform, () =>
            {
                if (_target != null)
                {
                    Destroy(_target.gameObject);
                    Hide();
                }
            });
        }

        public void Show(Rigidbody2D target, Vector2 screenPos)
        {
            _target = target;
            transform.position = screenPos;
            gameObject.SetActive(true);

            // Sync UI values
            // Create a unique material instance if shared to avoid affecting all objects
            // In a real app we'd manage materials better. Here we just clone it.
            if (_target.sharedMaterial == null)
            {
                _target.sharedMaterial = new PhysicsMaterial2D("Custom");
            }
            else if (_target.sharedMaterial.name != "Custom (Instance)") // Check if already unique-ish
            {
                var copy = new PhysicsMaterial2D("Custom");
                copy.friction = _target.sharedMaterial.friction;
                copy.bounciness = _target.sharedMaterial.bounciness;
                _target.sharedMaterial = copy;
            }

            if (_frictionSlider != null)
            {
                _frictionSlider.value = _target.sharedMaterial.friction;
            }

            if (_bouncinessSlider != null)
            {
                _bouncinessSlider.value = _target.sharedMaterial.bounciness;
            }

            if (_massSlider != null)
            {
                _massSlider.value = _target.mass;
            }
        }

        public void Hide()
        {
            gameObject.SetActive(false);
            _target = null;
        }
    }
}
