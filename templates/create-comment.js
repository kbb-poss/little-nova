var parse_json = function(data) {
    var return_json = {};
    for (index = 0; index < data.length; index++) {
        return_json[data[index].name] = data[index].value;
    }
    return return_json;
}

$(document).ready(function() {
    $('#send-comment').submit(function(event) {
        // Cancel sending in HTML 
        event.preventDefault();

        // Get the UTC (ISO format)
        var utc = new Date().toISOString();
        
        // Enter UTC
        $('#utc').val(utc);
        
        // Get the form element to be operated 
        var form = $(this);

        // Get the submit button 
        var button = form.find('#submit-comment');

        // Get input information 
        var array = form.serializeArray();
        console.log(array);

        // Convert to appropriate Json 
        var data = parse_json(array);
        console.log(data);

        // Send
        $.ajax({
            url: form.attr('action'),
            type: form.attr('method'),
            contentType: 'application/json',
            dataType: "json",
            data: JSON.stringify(data),
            timeout: 10000,  // milliseconds 
    
            // Before send
            beforeSend: function(xhr, settings) {
                // Disable the button to prevent double transmission 
                console.log(data);
                button.attr('disabled', true);
            },

            // After response 
            complete: function(xhr, text_status) {
                // Enable button and allow resend
                button.attr('disabled', false);
            },
            
            // Processing when communication is successful 
            success: function(result, text_status, xhr) {
                // Initialize input value 
                form[0].reset();
                alert('The comment was sent successfully');
            },
    
            // Processing when communication fails 
            error: function(xhr, text_status, error) {
                alert('Failed to send comment');
            }
        });
    });   
});