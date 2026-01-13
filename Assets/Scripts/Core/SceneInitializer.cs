using UnityEngine;
using UnityEngine.EventSystems;

namespace Core
{
    public static class SceneInitializer
    {
        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.AfterSceneLoad)]
        private static void InitializeScene()
        {
            // Create a root GameObject for our Managers
            var managersObject = new GameObject("Managers");
            UnityEngine.Object.DontDestroyOnLoad(managersObject);

            // Ensure we have an EventSystem for UI
            if (UnityEngine.Object.FindFirstObjectByType<EventSystem>() == null)
            {
                var eventSystemGo = new GameObject("EventSystem");
                eventSystemGo.AddComponent<EventSystem>();
                eventSystemGo.AddComponent<StandaloneInputModule>();
                eventSystemGo.transform.SetParent(managersObject.transform);
            }

            // Ensure we have a Main Camera
            var mainCam = UnityEngine.Object.FindFirstObjectByType<Camera>();
            if (mainCam == null)
            {
                var cameraGo = new GameObject("Main Camera");
                mainCam = cameraGo.AddComponent<Camera>();
                mainCam.orthographic = true;
                mainCam.orthographicSize = 10;
                mainCam.clearFlags = CameraClearFlags.SolidColor;
                mainCam.backgroundColor = new Color(0.2f, 0.2f, 0.2f); // Dark background like Algodoo
                cameraGo.tag = "MainCamera";
                cameraGo.transform.position = new Vector3(0, 0, -10);
            }

            // Ensure CameraController is attached
            if (mainCam.gameObject.GetComponent<CameraController>() == null)
            {
                mainCam.gameObject.AddComponent<CameraController>();
            }

            // Add Managers (We will add the components here as we create them)
            managersObject.AddComponent<SimulationManager>();
            managersObject.AddComponent<CommandManager>();
            var toolManager = managersObject.AddComponent<Tools.ToolManager>();
            managersObject.AddComponent<UI.UIManager>();

            // Register Tools
            // We need to add the tool components to the managersObject or a child
            toolManager.RegisterTool(managersObject.AddComponent<Tools.MoveTool>());
            toolManager.RegisterTool(managersObject.AddComponent<Tools.BoxTool>());
            toolManager.RegisterTool(managersObject.AddComponent<Tools.CircleTool>());
            toolManager.RegisterTool(managersObject.AddComponent<Tools.HingeTool>());
            toolManager.RegisterTool(managersObject.AddComponent<Tools.SpringTool>());

            Debug.Log("Scene Initialized via SceneInitializer");
        }
    }
}
