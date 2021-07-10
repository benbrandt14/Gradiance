using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.EventSystems;

public class MoveTool : MonoBehaviour
{

    Rigidbody2D rb;

    Vector2 prevPos;

    // Start is called before the first frame update
    void Start()
    {
        rb = GetComponent<Rigidbody2D>();
    }

    // Update is called once per frame
    void OnMouseOver()
    {

        var mousePos = (Vector2)Camera.main.ScreenToWorldPoint(Input.mousePosition);

        var click = Input.GetMouseButton(0) &&
                    !EventSystem.current.IsPointerOverGameObject();

        // var xOffset, yOffset = x,y difference between mousePos and GameObject centroid

        if (click)
        {
            //TODO transform object to mousePos + click offset
            transform.position = mousePos;

            rb.isKinematic = true;

            rb.freezeRotation = true;
        }
    }

    void OnMouseExit()
    {
        //TODO: make these enable as soon as click released (if a problem)
        //TODO: impart a final velocity (equal to held velocity) upon release (to 'throw')
        rb.isKinematic = false;
        rb.freezeRotation = false;
    }
}